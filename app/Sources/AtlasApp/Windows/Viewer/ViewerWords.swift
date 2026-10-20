import AtlasKit

/// The viewer's words that depend on its state: its mode labels, the picture's freshness, the footer's hint, the
/// control button and the two toggles, the recovery alert's button, the sidebar's facts and every notice.
///
/// The rule every window follows: fixed copy stays in AtlasUI's views; a presenter supplies only the words that
/// depend on state, interpolated, chosen by a condition or derived from facts, and a list row's fixed name travels
/// with its value. Words the viewer shares with another window are AtlasKit's ``Words``.
enum ViewerWords {
  /// The header while the picture is paused.
  static let paused = "Paused"
  /// The header while the picture is lost.
  static let reconnecting = "Reconnecting"
  /// The header before the first reading.
  static let connecting = "Connecting"
  /// The helper's name when the status gives none.
  static let helper = "Helper"
  /// The picture's freshness while paused.
  static let picturePaused = "Picture paused"
  /// The picture's freshness while it is lost.
  static let noPicture = "No picture"
  /// In control, with the desktop taking the keyboard.
  static let keyboardConnected = "Your mouse and keyboard go to the helper."
  /// In control, before the desktop takes the keyboard.
  static let clickToConnect = "Click the helper's screen to connect your keyboard."
  /// While control is asked for: the hint and the control button's title.
  static let connectingControl = "Connecting…"
  /// While the picture is paused.
  static let pausedHint = "Picture paused. Work in the helper continues."
  /// While the picture is lost.
  static let waitingForPicture = "Control returns once the helper's screen is back."
  /// With a picture, while the helper's input is not ready.
  static let inputNotReady = "Mouse and keyboard aren't ready in the helper yet."
  /// Watching, with nothing in the way.
  static let watching = "Watching. Control-Shift-Escape takes control."
  /// The control button in control.
  static let giveBack = "Give Back"
  /// The control button while watching.
  static let takeControl = "Take Control"
  /// The control button's tooltip in control.
  static let giveBackHelp = "Give control back (\(KeyMap.releaseSymbols))"
  /// The control button's tooltip while watching.
  static let takeControlHelp = "Use your mouse and keyboard in the helper (\(KeyMap.releaseSymbols))"
  /// The sidebar's toggle, for VoiceOver, while the sidebar shows.
  static let hideSidebar = "Hide sidebar"
  /// The sidebar's toggle, for VoiceOver, while it is hidden.
  static let showSidebar = "Show sidebar"
  /// The sidebar toggle's tooltip while the sidebar shows.
  static let hideSidebarHelp = "Hide Sidebar (⌥⌘I)"
  /// The sidebar toggle's tooltip while it is hidden.
  static let showSidebarHelp = "Show Sidebar (⌥⌘I)"
  /// The pause button, for VoiceOver, while the picture is paused.
  static let resumePicture = "Resume picture"
  /// The pause button, for VoiceOver, while pictures arrive.
  static let pausePicture = "Pause picture"
  /// The pause button's tooltip while the picture is paused.
  static let resumePictureHelp = "Resume Picture (⇧⌘P)"
  /// The pause button's tooltip while pictures arrive.
  static let pausePictureHelp = "Pause Picture (⇧⌘P)"
  /// The recovery alert's button.
  static let recover = "Recover"
  /// The recovery alert's button while recovery runs.
  static let recoveringButton = "Recovering…"
  /// The picture's note: pictures are never saved.
  static let pictureMemory = "Pictures of the helper stay in memory and are never saved."

  /// The input fact's name.
  static let inputFact = "Input"
  /// The control fact's name.
  static let controlFact = "Control"
  /// The pending actions fact's name.
  static let pendingFact = "Pending actions"

  // MARK: Notices

  /// The helper's session or display changed, which ended control.
  static let helperChanged = "The helper changed. Take control again when ready."
  /// The status said input is not ready, which ended control.
  static let inputUnavailable = "Mouse and keyboard permission is unavailable."
  /// A stale picture ended control.
  static let stalePicture = "The picture stopped updating. Wait for a fresh one, then take control again."
  /// Typed text held a grapheme longer than the helper takes in one go.
  static let sequenceTooLong = "This text contains a character sequence too long to type."

  /// A paste longer than the helper takes at once.
  ///
  /// - Parameter limit: The most Unicode characters one paste may carry.
  /// - Returns: "Paste is limited to 2,048 characters at a time; an emoji or an accent can count as more than one."
  static func pasteTooLong(_ limit: Int) -> String {
    "Paste is limited to \(limit.formatted()) characters at a time; an emoji or an accent can count as more than one."
  }

  /// The header's line after the status: the helper, then how fresh its picture is.
  ///
  /// - Parameters:
  ///   - account: The helper as people know it.
  ///   - freshness: How fresh the picture is.
  /// - Returns: "Atlas · Live".
  static func headerDetail(account: String, freshness: String) -> String {
    "\(account) · \(freshness)"
  }

  /// The picture's note with its size.
  ///
  /// - Parameters:
  ///   - width: The picture's width in pixels.
  ///   - height: Its height in pixels.
  /// - Returns: "3024 × 1964 pixels. Pictures of the helper stay in memory and are never saved."
  static func pictureNote(width: Int, height: Int) -> String {
    "\(width) × \(height) pixels. \(pictureMemory)"
  }
}
