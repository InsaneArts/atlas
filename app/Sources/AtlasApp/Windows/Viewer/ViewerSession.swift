/// What the viewer window is doing: watching or paused, recovering, holding control, and what the input method is
/// composing. Each is observed, so the window follows every change at once, Not Now's included.
struct ViewerSession {
  /// Whether the picture is paused.
  var paused = false
  /// Whether this window is running recovery now.
  var recovering = false
  /// One line about the last thing that happened, until the next.
  var notice: String?
  /// The recovery alert set aside with Not Now, until its reason changes.
  var recoveryLater: RecoveryKey?
  /// Text the input method is composing, not yet sent.
  var composing = ""
  /// The desktop takes the keyboard (while in control).
  var keyboardConnected = false
  /// The window is in full screen (its title bar buttons are hidden).
  var fullScreen = false
  /// The lease this window holds while in control.
  var lease: String?
  /// Whether an acquisition is under way.
  var acquiring = false

  /// Whether this window holds control.
  var holdingControl: Bool { lease != nil }
}
