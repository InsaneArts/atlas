import AppKit
import AtlasCommands

/// `--notice TITLE MESSAGE [--switch-back ACCOUNT]`: tells the person something in this session (the helper's, when
/// its daemon asks), waits for OK, then exits. With an account, its default button locks this session so they can
/// switch back to that account.
enum NoticeAlert {
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
    let alert = NSAlert()
    alert.messageText = title
    alert.informativeText = message
    if switchBack != nil {
      alert.addButton(withTitle: "Switch Back")
      let stay = alert.addButton(withTitle: "Stay Here")
      stay.keyEquivalent = "\u{1b}"
    } else {
      alert.addButton(withTitle: "OK")
    }
    // Finish launching first, so the request to come forward is honoured
    // and the notice has keyboard focus (Return closes it).
    app.finishLaunching()
    app.activate(ignoringOtherApps: true)
    alert.window.level = .floating
    if alert.runModal() == .alertFirstButtonReturn, let switchBack {
      // Run to its end before exiting: the notice runs before any app does. Should it fail, the person switches back
      // from the menu bar as usual; the notice has nothing more to say.
      _ = commands.runNow(.switchTo(switchBack))
    }
    exit(0)
  }
}
