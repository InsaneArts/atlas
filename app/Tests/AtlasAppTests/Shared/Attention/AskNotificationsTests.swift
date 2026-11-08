import AtlasService
import AtlasServiceTesting
import Foundation
import Testing
import UserNotifications

@testable import AtlasApp

@Suite("AskNotifications")
@MainActor
struct AskNotificationsTests {
  /// What the stand-in notifications were asked to do.
  final class Recorded {
    /// The notes posted.
    var posted: [AskNote] = []
    /// The ids taken away.
    var withdrawn: [String] = []
    /// How many times macOS was asked.
    var asked = 0
    /// What macOS answers.
    var allows = false

    /// The notifications that record here.
    var notifications: AskNotifications {
      AskNotifications(
        requestConsent: { [self] in
          asked += 1
          return allows
        },
        consent: { [self] in asked > 0 ? allows : nil },
        post: { [self] in posted.append($0) },
        withdraw: { [self] in withdrawn += $0 }
      )
    }
  }

  /// A question.
  ///
  /// - Parameters:
  ///   - id: Its id.
  ///   - once: Whether Just This Once is offered.
  /// - Returns: The question.
  static func ask(_ id: String, once: Bool = true) -> Ask {
    let asked = Date(timeIntervalSince1970: 1_762_600_000)
    return Ask(
      id: id,
      who: "Claude Code · ship-fix",
      task: "ship the setup fix",
      account: "github.com",
      kind: .click,
      element: "Merge pull request",
      offersOnce: once,
      asked: asked,
      ends: asked + 120
    )
  }

  @Test("a question's kind carries an action for each answer it offers, Allow for Task first")
  func theCategoryHoldsTheOfferedAnswers() throws {
    let categories = AskNotifications.categories()
    #expect(categories.count == 4)
    let all = try #require(categories.first { $0.identifier == AskNotifications.category(task: true, once: true) })
    #expect(all.actions.map(\.title) == ["Allow for Task", "Just This Once", "Deny"])
    #expect(all.actions.map(\.identifier).compactMap(AskNotifications.answer(of:)) == [.task, .once, .deny])
    #expect(all.actions.last?.options.contains(.destructive) == true)
    let taskOnly = try #require(
      categories.first { $0.identifier == AskNotifications.category(task: true, once: false) }
    )
    #expect(taskOnly.actions.map(\.title) == ["Allow for Task", "Deny"])
    #expect(AskNotifications.note(Self.ask("a1", once: false)).category == taskOnly.identifier)
    #expect(AskNotifications.note(Self.ask("a1")).title == "Click “Merge pull request” in github.com?")
  }

  @Test("an answer chosen in a notification answers the question it carries")
  func anActionAnswersItsQuestion() {
    let task = AskNotifications.action(AskAnswer.task)
    #expect(NotificationClicks.answer(to: task, ask: "a1") == .answer("a1", .task))
    #expect(NotificationClicks.answer(to: UNNotificationDefaultActionIdentifier, ask: "a1") == .open)
    #expect(NotificationClicks.answer(to: task) == .open, "an answer with no question only opens Atlas")
  }

  @Test("each new question posts once while the panel is closed, and goes once it is answered")
  func eachQuestionPostsOnce() {
    let recorded = Recorded()
    var panelOpen = false
    let signals = AskSignals(
      notifications: recorded.notifications,
      preferences: SettingsPreferences(
        loadTab: { nil },
        saveTab: { _ in },
        loadNotify: { false },
        saveNotify: { _ in }
      ),
      store: AskStore(service: ScriptedService()),
      panelOpen: { panelOpen }
    )
    signals.asksChanged(AskList(asks: [Self.ask("a1")]))
    signals.asksChanged(AskList(asks: [Self.ask("a1")]))
    #expect(recorded.posted.map(\.id) == ["a1"], "whatever Notify me says")
    panelOpen = true
    signals.asksChanged(AskList(asks: [Self.ask("a1"), Self.ask("a2")]))
    #expect(recorded.posted.map(\.id) == ["a1"], "the open panel shows it")
    signals.asksChanged(AskList())
    #expect(recorded.withdrawn == ["a1"])
  }

  @Test("macOS is asked once, when the first account joins the list, and a no badges the menu bar")
  func consentIsAskedOnce() async throws {
    let recorded = Recorded()
    var asked = false
    let store = AskStore(service: ScriptedService())
    var preferences = SettingsPreferences(
      loadTab: { nil },
      saveTab: { _ in },
      loadNotify: { false },
      saveNotify: { _ in }
    )
    preferences.loadAskConsentAsked = { asked }
    preferences.saveAskConsentAsked = { asked = true }
    let signals = AskSignals(notifications: recorded.notifications, preferences: preferences, store: store) { false }
    await signals.statusRead(try status(["overview": ["accounts": [] as [Any]]]))
    #expect(recorded.asked == 0, "no account, no question")
    let listed = try status(["overview": ["accounts": [["kind": "app", "id": "x", "name": "Slack", "added": 1]]]])
    await signals.statusRead(listed)
    await signals.statusRead(listed)
    #expect(recorded.asked == 1)
    #expect(store.notificationsRefused)
  }
}
