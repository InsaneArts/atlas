import AtlasService
import AtlasUI
import Foundation

/// What the Screenshots pane last read, the screenshot selected, the one shown large and how many stand in a row.
struct ViewerShotsFacts {
  /// The screenshots, newest first; nil before the first reading. The Activity pane reads them too, for its
  /// screenshot lines.
  var shots: [Screenshot]?
  /// The folder the service says they are kept in; nil while unknown, which leaves Show in Finder waiting.
  var folder: URL?
  /// Why the last reading failed, while it has; nil once one worked.
  var failure: String?
  /// The screenshot selected, by name; nil selects the first listed, unless the selection was cleared.
  var selected: String?
  /// Whether Esc cleared the selection, so none is ringed until one is picked or stepped to.
  var selectionCleared = false
  /// Keyboard moves made, so only a deliberate move asks the grid to scroll.
  var scrollRequest = 0
  /// The column to return to after crossing a short row; reset by a click or a horizontal move.
  var navigationColumn: Int?
  /// The screenshot shown large, by name; nil shows none.
  var large: String?
  /// The large screenshot as taken, once read.
  var full: ViewerFullShot?
  /// How many screenshots stand in each of the grid's rows, as it last said; ↑ and ↓ step by it.
  var columns = 1

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
