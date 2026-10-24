import AtlasStatus
import Foundation
import UserNotifications

/// How Atlas reaches the person outside its windows: asking macOS, the normal way, whether it may, and posting what
/// it tells them. A seam of closures, so tests stand in one that records and never reach Notification Center.
struct Notifier {
  /// Asks macOS whether Atlas may notify the person. macOS asks them the first time and remembers their answer, so
  /// asking again only reads it.
  var requestAuthorization: @MainActor () -> Void
  /// Posts what Atlas tells the person; macOS drops it if they said no.
  var post: @MainActor (RecoveryNotice) -> Void
  /// What opens the main window on a click, kept here because Notification Center holds its delegate weakly; nil
  /// for one that posts nowhere.
  var clicks: NotificationClicks?

  /// One that does nothing.
  static var silent: Notifier { Notifier(requestAuthorization: {}, post: { _ in }) }

  /// The app's own, through Notification Center, opening the main window when the person clicks a notification.
  /// Notification Center needs an app bundle and stops a process that has none, so a development build run from the
  /// command line, or a test, gets ``silent``.
  ///
  /// - Parameter open: Opens the main window.
  /// - Returns: The notifier.
  @MainActor static func live(open: @escaping @MainActor @Sendable () -> Void) -> Notifier {
    let bundle = Bundle.main
    guard bundle.bundlePath.hasSuffix(".app"), bundle.bundleIdentifier != nil else { return silent }
    let clicks = NotificationClicks(open: open)
    UNUserNotificationCenter.current().delegate = clicks
    return Notifier(
      requestAuthorization: {
        UNUserNotificationCenter.current()
          .requestAuthorization(options: [.alert, .sound]) { _, _ in }
      },
      post: { notice in
        let words = NotificationWords.note(notice)
        let content = UNMutableNotificationContent()
        content.title = words.title
        content.body = words.body
        content.sound = .default
        // One identifier per kind: a newer notice replaces the older one still in Notification Center.
        let request = UNNotificationRequest(identifier: identifier(notice), content: content, trigger: nil)
        UNUserNotificationCenter.current()
          .add(request, withCompletionHandler: nil)
      },
      clicks: clicks
    )
  }

  /// The notification's identifier for a kind of notice.
  ///
  /// - Parameter notice: The notice.
  /// - Returns: Its identifier.
  private static func identifier(_ notice: RecoveryNotice) -> String {
    switch notice {
    // Both kinds of Needs you share one: the newer replaces the older.
    case .unfinished, .inputStopped: "atlas.needs-you"
    case .loggedOut: "atlas.logged-out"
    }
  }
}
