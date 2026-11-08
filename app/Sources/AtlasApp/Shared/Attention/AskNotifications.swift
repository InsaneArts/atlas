import AtlasService
import AtlasUI
import Foundation
import UserNotifications

/// The notifications for the questions Atlas asks the person before an agent sends, apart from Notify me: asking
/// macOS, the normal way, whether Atlas may notify them, posting a question with the answers it offers as actions,
/// and taking one away once it is answered or ends. Allow for This Task leads, so macOS shows it as the button and the
/// rest under Options. A seam of closures, so tests stand in one that records and never reach Notification Center.
struct AskNotifications {
  /// Asks macOS whether Atlas may notify the person; macOS asks them the first time and remembers their answer.
  /// Returns whether it may.
  var requestConsent: @MainActor () async -> Bool
  /// Whether Atlas may notify the person, as macOS last said; nil before they were asked.
  var consent: @MainActor () async -> Bool?
  /// Posts a question.
  var post: @MainActor (AskNote) -> Void
  /// Takes the questions with these ids out of Notification Center.
  var withdraw: @MainActor ([String]) -> Void

  /// One that does nothing, where Notification Center can't be reached.
  static var silent: AskNotifications {
    AskNotifications(requestConsent: { false }, consent: { nil }, post: { _ in }, withdraw: { _ in })
  }

  /// The prefix of every question's action and kind.
  nonisolated static let prefix = "atlas.ask."
  /// The key under which a question's notification carries its id.
  nonisolated static let idKey = "ask"

  /// The app's own, through Notification Center; ``Notifier/live(open:recover:viewerShows:answer:)`` registers the
  /// kinds and answers the actions. Notification Center needs an app bundle, so a build run from the command line,
  /// or a test, gets ``silent``.
  ///
  /// - Returns: The notifications.
  static func live() -> AskNotifications {
    let bundle = Bundle.main
    guard bundle.bundlePath.hasSuffix(".app"), bundle.bundleIdentifier != nil else { return silent }
    return AskNotifications(
      requestConsent: {
        (try? await UNUserNotificationCenter.current()
          .requestAuthorization(options: [.alert, .sound])) ?? false
      },
      consent: {
        switch await UNUserNotificationCenter.current().notificationSettings().authorizationStatus {
        case .notDetermined: nil
        case .denied: false
        default: true
        }
      },
      post: { note in
        let content = UNMutableNotificationContent()
        content.title = note.title
        content.body = note.body
        content.sound = .default
        content.categoryIdentifier = note.category
        content.userInfo = [idKey: note.id]
        let request = UNNotificationRequest(identifier: identifier(note.id), content: content, trigger: nil)
        UNUserNotificationCenter.current()
          .add(request, withCompletionHandler: nil)
      },
      withdraw: { ids in
        let center = UNUserNotificationCenter.current()
        center.removeDeliveredNotifications(withIdentifiers: ids.map(identifier))
        center.removePendingNotificationRequests(withIdentifiers: ids.map(identifier))
      }
    )
  }

  /// A question's notification identifier.
  ///
  /// - Parameter id: The question's id.
  /// - Returns: The identifier.
  static func identifier(_ id: String) -> String {
    prefix + id
  }

  /// The action that gives an answer: Allow for Task, Just This Once and Deny.
  ///
  /// - Parameter answer: The answer.
  /// - Returns: The action's identifier.
  static func action(_ answer: AskAnswer) -> String {
    prefix + answer.rawValue
  }

  /// The answer an action gives, or nil for an action that is none of the questions'.
  ///
  /// - Parameter action: The action's identifier.
  /// - Returns: The answer.
  nonisolated static func answer(of action: String) -> AskAnswer? {
    guard action.hasPrefix(prefix) else { return nil }
    return AskAnswer(rawValue: String(action.dropFirst(prefix.count)))
  }

  /// The kind of a question that offers these answers; Deny always.
  ///
  /// - Parameters:
  ///   - task: Whether Allow for This Task is offered.
  ///   - once: Whether Just This Once is offered.
  /// - Returns: The kind's identifier.
  static func category(task: Bool, once: Bool) -> String {
    prefix
      + answers(task: task, once: once)
      .map(\.rawValue)
      .joined(separator: "-")
  }

  /// The answers a question offers, in order: Allow for This Task, Just This Once, then Deny, always.
  ///
  /// - Parameters:
  ///   - task: Whether Allow for This Task is offered.
  ///   - once: Whether Just This Once is offered.
  /// - Returns: The answers.
  static func answers(task: Bool, once: Bool) -> [AskAnswer] {
    (task ? [.task] : []) + (once ? [.once] : []) + [.deny]
  }

  /// Every kind of question, one for each set of answers it may offer, each answer an action: Allow for Task and
  /// Just This Once once the Mac is unlocked, Deny marked destructive.
  ///
  /// - Returns: The kinds.
  static func categories() -> [UNNotificationCategory] {
    [(true, true), (true, false), (false, true), (false, false)]
      .map { task, once in
        UNNotificationCategory(
          identifier: category(task: task, once: once),
          actions: answers(task: task, once: once).map(notificationAction),
          intentIdentifiers: []
        )
      }
  }

  /// An answer's action.
  ///
  /// - Parameter answer: The answer.
  /// - Returns: The action.
  static func notificationAction(_ answer: AskAnswer) -> UNNotificationAction {
    switch answer {
    case .task:
      UNNotificationAction(identifier: action(answer), title: "Allow for Task", options: [.authenticationRequired])
    case .once:
      UNNotificationAction(
        identifier: action(answer),
        title: AskChoice.once.title,
        options: [.authenticationRequired]
      )
    case .deny: UNNotificationAction(identifier: action(answer), title: AskChoice.deny.title, options: [.destructive])
    }
  }

  /// A question as its notification shows it.
  ///
  /// - Parameter ask: The question.
  /// - Returns: The note.
  static func note(_ ask: Ask) -> AskNote {
    AskNote(
      id: ask.id,
      title: "\(AskWords.title(ask))?",
      body: AskWords.notificationBody(ask),
      category: category(task: ask.offersTask, once: ask.offersOnce)
    )
  }
}
