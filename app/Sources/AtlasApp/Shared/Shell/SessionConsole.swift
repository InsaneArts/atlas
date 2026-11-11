import AppKit
import CoreGraphics
import Observation
import SystemConfiguration

/// Every switch and lock control follows this process's graphical session, never the account shown in a viewer.
@Observable
final class SessionConsole: NSObject {
  /// The process's one live session reading, observed by every window.
  static let shared = SessionConsole(read: currentSession, watching: true)
  /// Why an off-console lock never runs.
  static let refusal = "Switching accounts requires this session to be on this Mac's screen."

  /// Whether the latest session evidence puts this login on the physical console.
  private(set) var isOnConsole = false
  /// How many times this login was found off the screen after being on it, the moment macOS says so: a stay on
  /// screen is unbroken while the count holds, however soon the login came back.
  private(set) var departures = 0
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
    let now = read()
    if isOnConsole, !now { departures += 1 }
    isOnConsole = now
    return now
  }

  @objc private func sessionChanged(_ notification: Notification) {
    refresh()
  }

  private static func currentSession() -> Bool {
    let session = CGSessionCopyCurrentDictionary() as? [String: Any]
    return permitsSwitch(session: session, uid: getuid())
  }

  /// Missing evidence, a dictionary belonging to another login, or a locked screen never enables a lock: the person
  /// is using this login only once it is on screen and unlocked, as after a switch back once they have unlocked it.
  static func permitsSwitch(session: [String: Any]?, uid: uid_t) -> Bool {
    session?[kCGSessionUserIDKey as String] as? UInt32 == uid
      && session?[kCGSessionOnConsoleKey as String] as? Bool == true
      && session?[screenLocked] as? Bool != true
  }

  /// The window server's key for a login whose screen is locked; it has no public name.
  static let screenLocked = "CGSSessionScreenIsLocked"

  /// The short name of the account whose session is on the Mac's screen now, read in this process: `loginwindow`
  /// while the login window has the screen, nil when it can't be read.
  static func consoleUser() -> String? {
    guard let name = SCDynamicStoreCopyConsoleUser(nil, nil, nil) else {
      return nil
    }
    return name as String
  }
}
