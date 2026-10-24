import CoreGraphics

/// The measures a ``GroupedList`` lays its rows out by, apart from the list's generic type so they can be read and
/// tested on their own.
enum GroupedListMetrics {
  /// 3.5: how many rows a scrolling list shows before it scrolls, so the half row says there is more (v3 D10).
  static let visibleRows: CGFloat = 3.5
  /// 0.22: how much of a scrolling list's height its bottom edge fades over (v3 `.group.scrolls`' mask, clear from
  /// 78%).
  static let scrollMask: CGFloat = 0.22

  /// The height a scrolling list stops growing at.
  ///
  /// - Parameter rowHeight: One row's height.
  /// - Returns: Three and a half rows.
  static func scrollHeight(rowHeight: CGFloat) -> CGFloat {
    rowHeight * visibleRows
  }

  /// Whether a list scrolls inside its group, rather than grows: when its rows are taller together than
  /// ``scrollHeight(rowHeight:)``.
  ///
  /// - Parameters:
  ///   - rowCount: How many rows it holds.
  ///   - rowHeight: One row's height.
  /// - Returns: Whether it scrolls.
  static func scrolls(rowCount: Int, rowHeight: CGFloat) -> Bool {
    CGFloat(rowCount) * rowHeight > scrollHeight(rowHeight: rowHeight)
  }
}
