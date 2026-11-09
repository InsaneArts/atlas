import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Observation
import Testing
import UserNotifications

@testable import AtlasApp

@Suite("AskNotifications")
@MainActor
struct AskNotificationsTests {
  /// What the stand-in notifications were asked to do, observable, so a test waits on it.
  @Observable
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
  /// When the questions were asked, which the stores read as now.
  static let asked = Date(timeIntervalSince1970: 1_762_600_000)

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

  @Test("each new question posts once unless the person sees it, and goes once it is answered")
  func eachQuestionPostsOnce() {
    let recorded = Recorded()
    var seen: String?
    let signals = AskSignals(
      notifications: recorded.notifications,
      preferences: SettingsPreferences(
        loadTab: { nil },
        saveTab: { _ in },
        loadNotify: { false },
        saveNotify: { _ in }
      ),
      store: AskStore(service: ScriptedService()),
      inView: { seen }
    )
    signals.resolve(true)
    signals.asksChanged(AskList(asks: [Self.ask("a1")]))
    signals.asksChanged(AskList(asks: [Self.ask("a1")]))
    #expect(recorded.posted.map(\.id) == ["a1"], "whatever Notify me says")
    // The panel or the Screen's banner shows a2; on Activity or Apps it wouldn't be in view and would post.
    seen = "a2"
    signals.asksChanged(AskList(asks: [Self.ask("a1"), Self.ask("a2"), Self.ask("a3")]))
    #expect(recorded.posted.map(\.id) == ["a1", "a3"], "only the question in view stays quiet")
    signals.asksChanged(AskList())
    #expect(recorded.withdrawn == ["a1", "a3"])
    #expect(NotificationClicks.presentation(ask: "a2", inView: "a2").isEmpty)
    #expect(
      NotificationClicks.presentation(ask: "a2", inView: nil) == [.banner, .sound],
      "a pane that doesn't show it"
    )
  }

  @Test("a question seen in the panel posts once the panel closes, if it is still up")
  func aQuestionPostsOnceThePanelCloses() async {
    let recorded = Recorded()
    let service = ScriptedService()
    service.askList = AskList(asks: [Self.ask("a1")])
    let store = AskStore(service: service) { Self.asked }
    await store.read()
    var panelOpen = true
    let signals = AskSignals(
      notifications: recorded.notifications,
      preferences: SettingsPreferences(
        loadTab: { nil },
        saveTab: { _ in },
        loadNotify: { false },
        saveNotify: { _ in }
      ),
      store: store
    ) { panelOpen ? "a1" : nil }
    signals.resolve(true)
    #expect(recorded.posted.isEmpty, "the open panel shows it")
    signals.lookedAway()
    #expect(recorded.posted.isEmpty, "still in view")
    panelOpen = false
    signals.lookedAway()
    signals.lookedAway()
    #expect(recorded.posted.map(\.id) == ["a1"], "posted once, as the panel closed")

    let answered = Recorded()
    let quiet = AskSignals(
      notifications: answered.notifications,
      preferences: SettingsPreferences(
        loadTab: { nil },
        saveTab: { _ in },
        loadNotify: { false },
        saveNotify: { _ in }
      ),
      store: AskStore(service: ScriptedService())
    ) { nil }
    quiet.resolve(true)
    quiet.lookedAway()
    #expect(answered.posted.isEmpty, "a question answered before the panel closed posts nothing")
  }

  @Test("a question seen over the Screen posts once the main window leaves the Screen, and not before")
  func aQuestionPostsOnceTheScreenIsLeft() async {
    let recorded = Recorded()
    let service = ScriptedService()
    service.askList = AskList(asks: [Self.ask("a1")])
    let store = AskStore(service: service) { Self.asked }
    await store.read()
    var signals: AskSignals?
    let viewer = ViewerController.scripted(service, placeChanged: { signals?.lookedAway() })
    signals = AskSignals(
      notifications: recorded.notifications,
      preferences: SettingsPreferences(
        loadTab: { nil },
        saveTab: { _ in },
        loadNotify: { false },
        saveNotify: { _ in }
      ),
      store: store
    ) { viewer.session.place == .screen ? "a1" : nil }
    signals?.resolve(true)
    #expect(recorded.posted.isEmpty, "the Screen's banner shows it")
    viewer.send(.show(.activity))
    #expect(recorded.posted.map(\.id) == ["a1"], "Activity doesn't show it")
    viewer.send(.show(.screen))
    viewer.send(.show(.activity))
    #expect(recorded.posted.map(\.id) == ["a1"], "posted once")
  }

  @Test("nothing posts before macOS lets Atlas notify, and every question still up posts once it does")
  func questionsWaitForConsent() async {
    let recorded = Recorded()
    let service = ScriptedService()
    service.askList = AskList(asks: [Self.ask("a1")])
    let store = AskStore(service: service) { Self.asked }
    await store.read()
    let signals = AskSignals(
      notifications: recorded.notifications,
      preferences: SettingsPreferences(
        loadTab: { nil },
        saveTab: { _ in },
        loadNotify: { false },
        saveNotify: { _ in }
      ),
      store: store
    ) { nil }
    signals.asksChanged(AskList(asks: [Self.ask("a1")]))
    #expect(recorded.posted.isEmpty && signals.posted.isEmpty, "undecided: nothing posts, nothing counts as posted")
    signals.resolve(false)
    signals.asksChanged(AskList(asks: [Self.ask("a1")]))
    #expect(recorded.posted.isEmpty && signals.posted.isEmpty, "refused: nothing posts")
    signals.resolve(true)
    #expect(recorded.posted.map(\.id) == ["a1"], "allowed: the question still up posts")
    #expect(signals.posted == ["a1"])
  }

  @Test("after the first reading, a notification an earlier launch left whose question is gone is withdrawn")
  func leftoverNotificationsAreWithdrawn() async throws {
    let recorded = Recorded()
    let service = ScriptedService()
    service.askList = AskList(asks: [Self.ask("a2")])
    let store = AskStore(service: service) { Self.asked }
    await store.read()
    var notifications = recorded.notifications
    notifications.delivered = { ["a1", "a2"] }
    let signals = AskSignals(
      notifications: notifications,
      preferences: SettingsPreferences(
        loadTab: { nil },
        saveTab: { _ in },
        loadNotify: { false },
        saveNotify: { _ in }
      ),
      store: store
    ) { nil }
    signals.asksChanged(try #require(store.list))
    try await settle("the leftover was never withdrawn") { recorded.withdrawn == ["a1"] }
  }

  @Test("a notification kept from an earlier launch is withdrawn once its question is answered")
  func aKeptNotificationIsWithdrawnLater() async throws {
    let recorded = Recorded()
    let service = ScriptedService()
    service.askList = AskList(asks: [Self.ask("a1")])
    let store = AskStore(service: service) { Self.asked }
    await store.read()
    var notifications = recorded.notifications
    notifications.delivered = { ["a1"] }
    let signals = AskSignals(
      notifications: notifications,
      preferences: SettingsPreferences(
        loadTab: { nil },
        saveTab: { _ in },
        loadNotify: { false },
        saveNotify: { _ in }
      ),
      store: store
    ) { "a1" }
    signals.asksChanged(try #require(store.list))
    await signals.withdrawLeftovers()
    #expect(signals.posted.contains("a1"), "the kept notification is tracked")
    #expect(recorded.withdrawn.isEmpty, "its question is still up")
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
    let signals = AskSignals(notifications: recorded.notifications, preferences: preferences, store: store) { nil }
    await signals.statusRead(try status(["overview": ["accounts": [] as [Any]]]))
    #expect(recorded.asked == 0, "no account, no question")
    let listed = try status(["overview": ["accounts": [["kind": "app", "id": "x", "name": "Slack", "added": 1]]]])
    await signals.statusRead(listed)
    await signals.statusRead(listed)
    #expect(recorded.asked == 1)
    #expect(store.notificationsRefused)
  }
}
