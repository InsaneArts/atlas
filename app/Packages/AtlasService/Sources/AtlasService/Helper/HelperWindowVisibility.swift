/// How much of a window shows on the helper's display: the window sampled on a grid of ``columns`` by ``rows``
/// points, each one counted while it lies on the display and under no window in front of it, by the windows'
/// front-to-back order. A window whose place in that order is unknown counts as hidden, as nothing says what covers
/// it. Pure, so the Apps list's picture follows only what the list read.
public enum HelperWindowVisibility {
  /// 16: the grid's points across a window.
  public static let columns = 16
  /// 10: the grid's points down a window.
  public static let rows = 10
  /// 0.9: the share of a window that must show before its picture is cut from the helper's screen.
  public static let enough = 0.9

  /// The share of `window` that shows.
  ///
  /// - Parameters:
  ///   - window: The window.
  ///   - windows: Every window that may cover it, its own app's and other apps' alike; itself among them or not.
  ///   - display: The display's size, in points.
  /// - Returns: From 0, wholly covered, off the display or of no size, to 1, wholly in view.
  public static func share(of window: HelperWindow, among windows: [HelperWindow], display: HelperDisplay) -> Double {
    guard let order = window.order, window.width > 0, window.height > 0 else { return 0 }
    let front = windows.filter { other in
      other.id != window.id && (other.order.map { $0 < order } ?? false)
    }
    var shown = 0
    for column in 0..<columns {
      for row in 0..<rows {
        let x = window.x + window.width * (Double(column) + 0.5) / Double(columns)
        let y = window.y + window.height * (Double(row) + 0.5) / Double(rows)
        guard display.holds(x: x, y: y), !front.contains(where: { $0.holds(x: x, y: y) }) else { continue }
        shown += 1
      }
    }
    return Double(shown) / Double(columns * rows)
  }

  /// Whether enough of `window` shows for its picture to be cut from the helper's screen: at least ``enough``.
  ///
  /// - Parameters:
  ///   - window: The window.
  ///   - windows: Every window that may cover it.
  ///   - display: The display's size, in points.
  /// - Returns: Whether it is in view.
  public static func isInView(_ window: HelperWindow, among windows: [HelperWindow], display: HelperDisplay) -> Bool {
    share(of: window, among: windows, display: display) >= enough
  }
}
