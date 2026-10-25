import AtlasStatus
import Foundation
import UserNotifications

/// How Atlas reaches the person outside its windows: asking macOS, the normal way, whether it may, and posting what
/// it tells them. The helper needs you carries one action, Recover, or Open Atlas when
/// ``NotificationWords/offersRecover`` is off; a click on it opens Atlas. A seam of closures, so tests stand in one that records and never reach
/// Notification Center.
struct Notifier {
  /// Asks macOS whether Atlas may notify the person. macOS asks them the first time and remembers their answer, so
  /// asking again only reads it.
  var requestAuthorization: @MainActor () -> Void
  /// Posts what Atlas tells the person; macOS drops it if they said no.
  var post: @MainActor (RecoveryNotice) -> Void
  /// Posts why a Recover the person started from a notification did not run, as Atlas needs you again.
  var postRefusal: @MainActor (String) -> Void = { _ in }
  /// Takes The helper needs you out of Notification Center, once nothing needs the person, so no stale one is acted
  /// on.
  var withdraw: @MainActor () -> Void = {}
  /// Takes The helper logged out out of Notification Center, once the helper is logged in again.
  var withdrawLoggedOut: @MainActor () -> Void = {}
  /// What answers a click or an action, kept here because Notification Center holds its delegate weakly; nil for one
  /// that posts nowhere.
  var clicks: NotificationClicks?

  /// One that does nothing.
  static var silent: Notifier { Notifier(requestAuthorization: {}, post: { _ in }) }

  /// The app's own, through Notification Center: a click or Open Atlas opens the main window, and Recover recovers
  /// the helper. While Atlas is in front, a notification still shows as a banner unless the main window is on screen
  /// to say it. Notification Center needs an app bundle and stops a process that has none, so a development build run
  /// from the command line, or a test, gets ``silent``.
  ///
  /// - Parameters:
  ///   - open: Opens the main window.
  ///   - recover: Recovers the helper as the alert's Recover does.
  ///   - viewerShows: Whether the main window is on screen now.
  /// - Returns: The notifier.
  static func live(
    open: @escaping @MainActor @Sendable () -> Void,
    recover: @escaping @MainActor @Sendable () async -> Void,
    viewerShows: @escaping @MainActor @Sendable () -> Bool
  ) -> Notifier {
    let bundle = Bundle.main
    guard bundle.bundlePath.hasSuffix(".app"), bundle.bundleIdentifier != nil else { return silent }
    let clicks = NotificationClicks(open: open, recover: recover, viewerShows: viewerShows)
    let center = UNUserNotificationCenter.current()
    center.delegate = clicks
    center.setNotificationCategories([attentionCategory()])
    return Notifier(
      requestAuthorization: {
        UNUserNotificationCenter.current()
          .requestAuthorization(options: [.alert, .sound]) { _, _ in }
      },
      post: { notice in
        deliver(NotificationWords.note(notice), identifier: identifier(notice))
      },
      postRefusal: { reason in
        deliver(NotificationWords.refusal(reason), identifier: needsYouIdentifier)
      },
      withdraw: {
        UNUserNotificationCenter.current()
          .removeDeliveredNotifications(withIdentifiers: [needsYouIdentifier])
      },
      withdrawLoggedOut: {
        UNUserNotificationCenter.current()
          .removeDeliveredNotifications(withIdentifiers: [loggedOutIdentifier])
      },
      clicks: clicks
    )
  }

  /// The helper needs you's kind: Recover alone, which runs without bringing Atlas forward but only once the Mac is
  /// unlocked; or Open Atlas alone, which does.
  ///
  /// - Parameter offersRecover: Whether Recover is offered (``NotificationWords/offersRecover``).
  /// - Returns: The category.
  static func attentionCategory(offersRecover: Bool = NotificationWords.offersRecover) -> UNNotificationCategory {
    let open = UNNotificationAction(
      identifier: NotificationWords.openAction,
      title: NotificationWords.openAtlas,
      options: [.foreground]
    )
    let recover = UNNotificationAction(
      identifier: NotificationWords.recoverAction,
      title: NotificationWords.recover,
      options: [.authenticationRequired]
    )
    return UNNotificationCategory(
      identifier: NotificationWords.attentionCategory,
      actions: offersRecover ? [recover] : [open],
      intentIdentifiers: []
    )
  }

  /// Posts one notification, with a sound unless its words are quiet, carrying The helper needs you's action when its
  /// words say so.
  ///
  /// - Parameters:
  ///   - note: Its words.
  ///   - identifier: Its identifier: a newer notice of the same kind replaces the older one still showing.
  private static func deliver(_ note: NotificationWords.Note, identifier: String) {
    let content = UNMutableNotificationContent()
    content.title = note.title
    content.body = note.body
    content.sound = note.sounds ? .default : nil
    if note.acts { content.categoryIdentifier = NotificationWords.attentionCategory }
    let request = UNNotificationRequest(identifier: identifier, content: content, trigger: nil)
    UNUserNotificationCenter.current()
      .add(request, withCompletionHandler: nil)
  }

  /// The helper needs you's one identifier, which both of its reasons and a refused Recover share: the newer replaces
  /// the older.
  private static let needsYouIdentifier = "atlas.needs-you"
  /// The helper logged out's identifier.
  private static let loggedOutIdentifier = "atlas.logged-out"

  /// The notification's identifier for a kind of notice.
  ///
  /// - Parameter notice: The notice.
  /// - Returns: Its identifier.
  private static func identifier(_ notice: RecoveryNotice) -> String {
    switch notice {
    case .unfinished, .inputStopped: needsYouIdentifier
    case .loggedOut: loggedOutIdentifier
    }
  }
}
