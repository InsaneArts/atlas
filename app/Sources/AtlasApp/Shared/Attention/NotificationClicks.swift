import Foundation
import UserNotifications

/// Answers the person's click on one of Atlas's notifications, or one of Atlas needs you's actions: a click or Open
/// Atlas opens the main window, Recover recovers the helper. A class only because Notification Center's delegate
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
    /// Nothing: the person dismissed it.
    case none
  }

  /// Opens the main window.
  private let open: @MainActor @Sendable () -> Void
  /// Recovers the helper as the alert's Recover does.
  private let recover: @MainActor @Sendable () async -> Void

  /// A delegate that opens the main window on a click and recovers the helper on Recover.
  ///
  /// - Parameters:
  ///   - open: Opens the main window.
  ///   - recover: Recovers the helper.
  init(open: @escaping @MainActor @Sendable () -> Void, recover: @escaping @MainActor @Sendable () async -> Void) {
    self.open = open
    self.recover = recover
  }

  /// What a response's action asks for: Recover recovers while Recover is offered; a dismissal does nothing; a click,
  /// Open Atlas, or a Recover left from when it was offered opens Atlas.
  ///
  /// - Parameters:
  ///   - action: The response's action identifier.
  ///   - offersRecover: Whether Recover is offered (``NotificationWords/offersRecover``).
  /// - Returns: What to do.
  static func answer(to action: String, offersRecover: Bool = NotificationWords.offersRecover) -> Answer {
    switch action {
    case NotificationWords.recoverAction where offersRecover: .recover
    case UNNotificationDismissActionIdentifier: .none
    default: .open
    }
  }

  /// The person clicked a notification or chose one of its actions: the main window opens on Screen, where Needs you
  /// shows its Recover alert, or Recover starts. Notification Center is answered at once; Recover runs on.
  ///
  /// - Parameters:
  ///   - center: The notification center.
  ///   - response: What the person did.
  func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
    switch Self.answer(to: response.actionIdentifier) {
    case .open: await open()
    case .recover:
      // Unstructured, so Notification Center is answered at once while the recovery, which may take a while, runs on.
      Task { await recover() }
    case .none: return
    }
  }

  /// A notification arrived while Atlas is in front: none shows, since its window or panel already says it.
  ///
  /// - Parameters:
  ///   - center: The notification center.
  ///   - notification: The notification.
  /// - Returns: No presentation.
  func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    willPresent notification: UNNotification
  ) async -> UNNotificationPresentationOptions {
    []
  }
}
