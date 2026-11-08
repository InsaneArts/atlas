/// What the picture sheet's footer briefly says after Copy or Show in Finder: Copy reading Copied with a checkmark
/// once the picture is on the clipboard, or a short line saying what could not be done.
public enum ViewerSheetNotice: Equatable, Sendable {
  /// The picture is on the clipboard.
  case copied
  /// The picture could not be read or put on the clipboard.
  case copyFailed
  /// The picture's file could not be found to show in the Finder.
  case fileMissing
}
