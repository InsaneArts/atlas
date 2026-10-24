import AtlasControl

/// The top row's words: the control button in each state and its tooltips, the picture's pause button, ⋯, the
/// Details button and the tabs.
extension ViewerWords {
  /// The control button while watching.
  static let takeControl = "Take Control"
  /// The control button in control.
  static let giveBack = "Give Back"
  /// The control button while control is asked for.
  static let connectingControl = "Connecting…"
  /// The control button while the helper needs the person.
  static let recoverButton = "Recover…"
  /// The control button's tooltip while watching.
  static let takeControlHelp = "Use your mouse and keyboard in the helper (\(KeyMap.releaseSymbols))"
  /// The control button's tooltip in control.
  static let giveBackHelp = "Give control back (\(KeyMap.releaseSymbols))"
  /// The control button's tooltip while the helper needs the person.
  static let recoverHelp = "Check the helper and start its mouse and keyboard again"
  /// The pause button, for VoiceOver, while the picture is paused.
  static let resumePicture = "Resume picture"
  /// The pause button, for VoiceOver, while pictures arrive.
  static let pausePicture = "Pause picture"
  /// The pause button's tooltip while the picture is paused.
  static let resumePictureHelp = "Resume Picture (⇧⌘P)"
  /// The pause button's tooltip while pictures arrive.
  static let pausePictureHelp = "Pause Picture (⇧⌘P)"
  /// ⋯, for VoiceOver.
  static let more = "More"
  /// ⋯'s tooltip.
  static let moreHelp = "The picture and the helper"
  /// The Details button, for VoiceOver, while Details shows.
  static let hideDetails = "Hide details"
  /// The Details button, for VoiceOver, while it is hidden.
  static let showDetails = "Show details"
  /// The Details button's tooltip while Details shows, and the View menu's item.
  static let hideDetailsHelp = "Hide Details (⌥⌘I)"
  /// The Details button's tooltip while it is hidden.
  static let showDetailsHelp = "Show Details (⌥⌘I)"
  /// The tabs, for VoiceOver.
  static let tabsLabel = "What you see"
}
