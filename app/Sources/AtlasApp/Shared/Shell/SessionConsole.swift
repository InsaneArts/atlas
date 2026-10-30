import AppKit
import CoreGraphics
import Observation

/// Every switch and lock control follows this process's graphical session, never the account shown in a viewer.
@Observable
final class SessionConsole: NSObject {
  /// The process's one live session reading, observed by every window.
  static let shared = SessionConsole(read: currentSession, watching: true)
  /// Why an off-console lock never runs.
  static let refusal = "Switching accounts requires this session to be on this Mac's screen."

  /// Whether the latest session evidence puts this login on the physical console.
  private(set) var isOnConsole = false
  @ObservationIgnored private let read: @MainActor () -> Bool

  /// Tests supply the session reading; only the shared instance watches macOS session changes.
  init(read: @escaping @MainActor () -> Bool, watching: Bool = false) {
    self.read = read
    super.init()
    refresh()
    if watching {
      for name in [NSWorkspace.sessionDidBecomeActiveNotification, NSWorkspace.sessionDidResignActiveNotification] {
        NSWorkspace.shared.notificationCenter
          .addObserver(self, selector: #selector(sessionChanged), name: name, object: nil)
      }
    }
  }

  /// Rechecks immediately before an action, including after any asynchronous cleanup.
  @discardableResult
  func refresh() -> Bool {
    isOnConsole = read()
    return isOnConsole
  }

  @objc private func sessionChanged(_ notification: Notification) {
    refresh()
  }

  private static func currentSession() -> Bool {
    let session = CGSessionCopyCurrentDictionary() as? [String: Any]
    return permitsSwitch(session: session, uid: getuid())
  }

  /// Missing evidence, or a dictionary belonging to another login, never enables a lock.
  static func permitsSwitch(session: [String: Any]?, uid: uid_t) -> Bool {
    session?[kCGSessionUserIDKey as String] as? UInt32 == uid
      && session?[kCGSessionOnConsoleKey as String] as? Bool == true
  }
}
