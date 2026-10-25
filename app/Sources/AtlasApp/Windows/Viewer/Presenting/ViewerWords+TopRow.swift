/// The top row's words: the control button in each state and its tooltips, the picture's pause button, ⋯, the
/// Details button and the tabs. VoiceOver reads each button by its menu item's exact title, and each tooltip is that
/// title with its shortcut (``ShortcutWords``).
extension ViewerWords {
  /// The control button while watching.
  static let takeControl = "Take Control"
  /// The control button in control.
  static let giveBack = "Give Back"
  /// The control button while control is asked for, or waits for a fresh picture or the agent's step.
  static let connectingControl = "Connecting…"
  /// The control button while the helper needs the person.
  static let recoverButton = "Recover…"
  /// Every title the control button can show, so it keeps the widest one's width.
  static let controlTitles = [takeControl, giveBack, connectingControl, recoverButton]
  /// The control button's tooltip while the helper needs the person.
  static let recoverHelp = "Check the helper and start its mouse and keyboard again"
  /// ⋯, for VoiceOver and its tooltip.
  static let more = "More"
  /// The tabs, for VoiceOver: the View menu they stand for.
  static let tabsLabel = "View"
}
