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
    /// Nothing: the person dismissed it.
    case none
  }

  /// Opens the main window.
  private let open: @MainActor @Sendable () -> Void
  /// Recovers the helper as the alert's Recover does.
  private let recover: @MainActor @Sendable () async -> Void
  /// Whether the main window is on screen now.
  private let viewerShows: @MainActor @Sendable () -> Bool

  /// A delegate that opens the main window on a click and recovers the helper on Recover.
  ///
  /// - Parameters:
  ///   - open: Opens the main window.
  ///   - recover: Recovers the helper.
  ///   - viewerShows: Whether the main window is on screen now; never, by default.
  init(
    open: @escaping @MainActor @Sendable () -> Void,
    recover: @escaping @MainActor @Sendable () async -> Void,
    viewerShows: @escaping @MainActor @Sendable () -> Bool = { false }
  ) {
    self.open = open
    self.recover = recover
    self.viewerShows = viewerShows
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
