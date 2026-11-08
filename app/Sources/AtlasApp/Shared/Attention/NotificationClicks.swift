import AtlasService
import Foundation
import UserNotifications

/// Answers the person's click on one of Atlas's notifications, or The helper needs you's action: a click or Open
/// Atlas opens the main window, Recover recovers the helper. While Atlas is in front, a notification shows as a banner
/// with its sound unless the main window is on screen to say it. A class only because Notification Center's delegate
/// must be an object.
/// `nonisolated`: Notification Center calls its delegate on a queue of its own, and only what it then does needs the
/// main actor.
nonisolated final class NotificationClicks: NSObject, UNUserNotificationCenterDelegate, Sendable {
  /// What a response asks for.
  enum Answer: Equatable {
    /// Open the main window.
    case open
    /// Recover the helper.
    case recover
    /// Answer a question Atlas asks the person.
    ///
    /// - Parameters:
    ///   - id: The question's id.
    ///   - answer: The answer.
    case answer(String, AskAnswer)
    /// Nothing: the person dismissed it.
    case none
  }

  /// Opens the main window.
  private let open: @MainActor @Sendable () -> Void
  /// Recovers the helper as the alert's Recover does.
  private let recover: @MainActor @Sendable () async -> Void
  /// Whether the main window is on screen now.
  private let viewerShows: @MainActor @Sendable () -> Bool
  /// Answers a question as the person.
  private let answerAsk: @MainActor @Sendable (String, AskAnswer) async -> Void

  /// A delegate that opens the main window on a click and recovers the helper on Recover.
  ///
  /// - Parameters:
  ///   - open: Opens the main window.
  ///   - recover: Recovers the helper.
  ///   - viewerShows: Whether the main window is on screen now; never, by default.
  ///   - answer: Answers a question as the person; nothing by default.
  init(
    open: @escaping @MainActor @Sendable () -> Void,
    recover: @escaping @MainActor @Sendable () async -> Void,
    viewerShows: @escaping @MainActor @Sendable () -> Bool = { false },
    answer: @escaping @MainActor @Sendable (String, AskAnswer) async -> Void = { _, _ in }
  ) {
    self.open = open
    self.recover = recover
    self.viewerShows = viewerShows
    self.answerAsk = answer
  }

  /// What a response's action asks for: Recover recovers; a question's answer, on the question it carries, answers it;
  /// a dismissal does nothing; a click, or any other action, opens Atlas.
  ///
  /// - Parameters:
  ///   - action: The response's action identifier.
  ///   - ask: The id of the question the notification carries, if it is one.
  /// - Returns: What to do.
  static func answer(to action: String, ask: String? = nil) -> Answer {
    if let ask, let answer = AskNotifications.answer(of: action) { return .answer(ask, answer) }
    return switch action {
    case NotificationWords.recoverAction: .recover
    case UNNotificationDismissActionIdentifier: .none
    default: .open
    }
  }

  /// The person clicked a notification or chose one of its actions: the main window opens on Screen, where Needs you
  /// shows its Recover alert, Recover starts, or a question is answered. Notification Center is answered at once; Recover runs on.
  ///
  /// - Parameters:
  ///   - center: The notification center.
  ///   - response: What the person did.
  func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
    let ask = response.notification.request.content.userInfo[AskNotifications.idKey] as? String
    switch Self.answer(to: response.actionIdentifier, ask: ask) {
    case .open: await open()
    case .answer(let id, let answer): await answerAsk(id, answer)
    case .recover:
      // Unstructured, so Notification Center is answered at once while the recovery, which may take a while, runs on.
      Task { await recover() }
    case .none: return
    }
  }

  /// A notification arrived while Atlas is in front: a banner with its sound, unless the main window is on screen,
  /// which already says it. With only Settings or the panel open, the person still hears.
  ///
  /// - Parameters:
  ///   - center: The notification center.
  ///   - notification: The notification.
  /// - Returns: How it shows.
  func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    willPresent notification: UNNotification
  ) async -> UNNotificationPresentationOptions {
    Self.presentation(viewerShows: await viewerShows())
  }

  /// How a notification shows while Atlas is in front.
  ///
  /// - Parameter viewerShows: Whether the main window is on screen now.
  /// - Returns: Nothing while the main window shows, else a banner with its sound.
  static func presentation(viewerShows: Bool) -> UNNotificationPresentationOptions {
    viewerShows ? [] : [.banner, .sound]
  }
}
