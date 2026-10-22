import Foundation
import UserNotifications

/// Opens Atlas's main window when the person clicks one of its notifications. A class only because Notification
/// Center's delegate must be an object.
final class NotificationClicks: NSObject, UNUserNotificationCenterDelegate, Sendable {
  /// Opens the main window.
  private let open: @MainActor @Sendable () -> Void

  /// A delegate that opens the main window on a click.
  ///
  /// - Parameter open: Opens the main window.
  init(open: @escaping @MainActor @Sendable () -> Void) {
    self.open = open
  }

  /// The person clicked a notification: the main window opens on Screen, where Needs you shows its Recover alert.
  ///
  /// - Parameters:
  ///   - center: The notification center.
  ///   - response: What the person did.
  func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
    await open()
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
