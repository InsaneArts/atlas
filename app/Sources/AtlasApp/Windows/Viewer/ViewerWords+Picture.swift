/// The ⋯ menu's words: how often pictures arrive, Copy and Save Picture and what follows them, and the helper's
/// note.
extension ViewerWords {
  /// Pictures as often as the window reads them.
  static let live = "Live"
  /// A picture every five seconds.
  static let everyFiveSeconds = "Every 5 seconds"
  /// No new pictures.
  static let paused = "Paused"
  /// The ⋯ menu's picture header.
  static let pictureHeader = "Picture"
  /// Copy Picture.
  static let copyPicture = "Copy Picture"
  /// Save Picture….
  static let savePicture = "Save Picture…"
  /// Once the picture is on the clipboard.
  static let pictureCopied = "Picture copied."
  /// A picture that could not be written.
  static let pictureNotSaved = "Atlas couldn't save the picture."

  /// The ⋯ menu's note: the helper and its display.
  ///
  /// - Parameters:
  ///   - account: The helper as people know it.
  ///   - display: Its display, when known.
  /// - Returns: "Helper “Atlas” · 2560 × 1440".
  static func helperNote(account: String, display: String?) -> String {
    let name = "Helper “\(account)”"
    return display.map { "\(name) · \($0)" } ?? name
  }
}
