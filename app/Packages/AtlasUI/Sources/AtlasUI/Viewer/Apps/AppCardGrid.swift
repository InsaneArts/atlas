import SwiftUI

/// A grid of app cards: as many columns as the width holds at ``ViewerLayout/appCardLeast`` each, from one up to
/// ``ViewerLayout/appCardColumns``, every column the same width, and every card in a row as tall as the row's
/// tallest, so a grid never looks ragged. Cards keep the order they are given.
struct AppCardGrid: Layout {
  /// The least width a card takes.
  var least = ViewerLayout.appCardLeast
  /// The most cards across.
  var most = ViewerLayout.appCardColumns
  /// Between cards, across and down.
  var gap = ViewerLayout.appCardGap

  /// How many columns a width holds.
  ///
  /// - Parameter width: The grid's width.
  /// - Returns: From 1 to ``most``.
  func columns(for width: CGFloat) -> Int {
    let fitting = Int(((width + gap) / (least + gap)).rounded(.down))
    return min(max(fitting, 1), most)
  }

  /// A column's width in a grid of `width`.
  ///
  /// - Parameter width: The grid's width.
  /// - Returns: The width each card takes.
  func columnWidth(for width: CGFloat) -> CGFloat {
    let columns = CGFloat(columns(for: width))
    return max((width - gap * (columns - 1)) / columns, .zero)
  }

  /// Each row's height: its tallest card's, at the column's width.
  ///
  /// - Parameters:
  ///   - width: The grid's width.
  ///   - subviews: The cards.
  /// - Returns: The rows' heights, top to bottom.
  func rowHeights(for width: CGFloat, _ subviews: Subviews) -> [CGFloat] {
    let columns = columns(for: width)
    let proposal = ProposedViewSize(width: columnWidth(for: width), height: nil)
    return stride(from: 0, to: subviews.count, by: columns)
      .map { start in
        subviews[start..<min(start + columns, subviews.count)]
          .map { $0.sizeThatFits(proposal).height }
          .max() ?? .zero
      }
  }

  /// The grid's size: the width it is offered, and its rows' heights with the gaps between them.
  func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
    let width = proposal.width ?? least
    let rows = rowHeights(for: width, subviews)
    let height = rows.reduce(.zero, +) + gap * CGFloat(max(rows.count - 1, 0))
    return CGSize(width: width, height: height)
  }

  /// Places each card in its column and row, at the column's width and the row's height.
  func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
    let columns = columns(for: bounds.width)
    let width = columnWidth(for: bounds.width)
    let rows = rowHeights(for: bounds.width, subviews)
    var top = bounds.minY
    for (row, height) in rows.enumerated() {
      for column in 0..<columns {
        let index = row * columns + column
        guard index < subviews.count else { break }
        let origin = CGPoint(x: bounds.minX + (width + gap) * CGFloat(column), y: top)
        subviews[index].place(at: origin, proposal: ProposedViewSize(width: width, height: height))
      }
      top += height + gap
    }
  }
}
