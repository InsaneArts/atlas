import AtlasKit
import AtlasUI
import Foundation

/// What the Screenshots tab last read, the search, the screenshot selected and the one shown large.
struct ViewerShotsFacts {
  /// The screenshots, newest first; nil before the first reading. The Activity tab reads them too, for its
  /// screenshot lines.
  var shots: [Screenshot]?
  /// The folder the service says they are kept in; nil while unknown, which leaves Show in Finder waiting.
  var folder: URL?
  /// Why the last reading failed, while it has; nil once one worked.
  var failure: String?
  /// The search by time, as typed.
  var query = ""
  /// The screenshot selected, by name; nil selects the first listed.
  var selected: String?
  /// The screenshot shown large, by name; nil shows none.
  var large: String?
  /// The large screenshot as taken, once read.
  var full: ViewerFullShot?

  /// The file of a listed screenshot, inside the folder the service names; nil for any other name, or while the
  /// folder is unknown.
  ///
  /// - Parameter name: The screenshot's file name.
  /// - Returns: The file.
  func file(named name: String) -> URL? {
    ScreenshotList(shots: shots ?? [], folder: folder)
      .file(named: name)
  }
}
