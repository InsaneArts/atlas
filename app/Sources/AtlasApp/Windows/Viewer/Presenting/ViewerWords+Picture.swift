/// The picture's commands' words: how often pictures arrive, Copy and Save for the picture and for a screenshot, and
/// what follows them. Every item is in title case, as the menus' are.
extension ViewerWords {
  /// Pictures as often as the window reads them.
  static let live = "Live"
  /// A picture every five seconds.
  static let everyFiveSeconds = "Every 5 Seconds"
  /// No new pictures.
  static let paused = "Paused"
  /// The submenu of how often pictures arrive, in the View menu and the stage's menu.
  static let updatesItem = "Picture Updates"
  /// Copy Picture.
  static let copyPicture = "Copy Picture"
  /// Save Picture….
  static let savePicture = "Save Picture…"
  /// Copy Screenshot, in the Screenshots list with one shown large or selected.
  static let copyScreenshot = "Copy Screenshot"
  /// Save Screenshot…, in the Screenshots list with one shown large or selected.
  static let saveScreenshot = "Save Screenshot…"
  /// Once the picture is on the clipboard.
  static let pictureCopied = "Picture copied."
  /// A picture that could not be written.
  static let pictureNotSaved = "Atlas couldn't save the picture."
  /// A screenshot that could not be written.
  static let shotNotSaved = "Atlas couldn't save the screenshot."

  /// A rate's item.
  ///
  /// - Parameter rate: The rate.
  /// - Returns: "Live" or "Every 5 Seconds".
  static func rate(_ rate: ViewerPictureRate) -> String {
    switch rate {
    case .live: live
    case .everyFiveSeconds: everyFiveSeconds
    }
  }
}
