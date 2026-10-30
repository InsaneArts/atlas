import AppKit
import AtlasCommands

/// `--notice TITLE MESSAGE [--switch-back ACCOUNT]`: tells the person something in this session (the helper's, when
/// its daemon asks), waits for an answer, then exits. It stays a standard system alert, with Atlas's real icon:
/// a custom window would add risk for a moment the person sees once. On-console with an account, Switch Back is the
/// default and Stay Here takes Escape. Off-console, or without an account, it offers OK alone.
enum NoticeAlert {
  /// The default button with an account: switch back to it.
  private static let switchBack = "Switch Back"
  /// The way out with an account: stay in this session.
  private static let stayHere = "Stay Here"
  /// The one button without an account.
  private static let ok = "OK"

  /// Shows the notice and exits when it is answered.
  ///
  /// - Parameters:
  ///   - title: The notice's title.
  ///   - message: What it says.
  ///   - switchBack: The account Switch Back locks the screen for, or nil for OK alone.
  ///   - commands: The `atlas` Switch Back runs.
  /// - Returns: Never; the process exits.
  static func show(title: String, message: String, switchBack: String?, commands: Commands) -> Never {
    let app = NSApplication.shared
    DockIcon.settle(windowOpen: false)
    let console = SessionConsole.shared
    let alert = make(title: title, message: message, switchBack: switchBack, onConsole: console.refresh())
    if switchBack != nil, alert.buttons.count > 1 {
      StateTracking.follow { [weak alert] in
        alert == nil ? nil : console.isOnConsole
      } apply: { [weak alert] onConsole in
        alert?.buttons.first?.isEnabled = onConsole
      }
    }
    // Finish launching first, so the request to come forward is honoured
    // and the notice has keyboard focus (Return closes it).
    app.finishLaunching()
    app.activate(ignoringOtherApps: true)
    alert.window.level = .floating
    if alert.runModal() == .alertFirstButtonReturn, alert.buttons.count > 1, let switchBack, console.refresh() {
      // Wait for the guarded CLI action before exiting; it independently rechecks console ownership.
      _ = commands.runNow(.switchTo(switchBack))
    }
    exit(0)
  }

  /// The notice as a system alert: Atlas's icon, the title and message, and Switch Back as the default (the first
  /// button, which Return presses) with Stay Here on Escape, or OK alone without an account. The renders draw it too.
  ///
  /// - Parameters:
  ///   - title: The notice's title.
  ///   - message: What it says.
  ///   - switchBack: The account Switch Back is for, or nil for OK alone.
  ///   - onConsole: Whether this session currently owns the physical console; absent evidence offers OK alone.
  /// - Returns: The alert, not yet shown.
  static func make(title: String, message: String, switchBack: String?, onConsole: Bool = false) -> NSAlert {
    let alert = NSAlert()
    alert.messageText = title
    alert.informativeText = message
    alert.icon = AppIconImage.icon
    if switchBack != nil, onConsole {
      alert.addButton(withTitle: Self.switchBack)
      let stay = alert.addButton(withTitle: Self.stayHere)
      stay.keyEquivalent = "\u{1b}"
    } else {
      alert.addButton(withTitle: Self.ok)
    }
    return alert
  }
}
