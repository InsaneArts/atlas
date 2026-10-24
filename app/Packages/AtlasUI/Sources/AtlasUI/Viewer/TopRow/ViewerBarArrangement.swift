import SwiftUI

/// The top row's three columns (v3 `.vtop`'s `minmax(0, 1fr) auto minmax(0, 1fr)`): the tabs centred on the window, the
/// status from the leading edge and the buttons to the trailing edge, each side column as wide as the other. Its
/// ideal width is what the row needs for both sides to hold their own content, so ``ViewThatFits`` can tell whether a
/// layout fits: the wider side counts twice, as the tabs stay centred. Should even the narrowest layout not fit, the
/// tabs move off centre, never under the buttons. Its subviews are the status, the tabs and the buttons, in that order;
/// the status takes the room left of the tabs and cuts its line short within it.
struct ViewerBarArrangement: Layout {
  /// Between the side columns and the tabs.
  let gap: CGFloat

  /// The row's size: the width offered, or, asked for its ideal, the width its layout needs with the tabs centred.
  ///
  /// - Parameters:
  ///   - proposal: The width and height offered.
  ///   - subviews: The status, the tabs and the buttons.
  ///   - cache: Unused.
  /// - Returns: The size.
  func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
    let tallest = subviews.map { $0.sizeThatFits(.unspecified).height }
      .max()
    let height = proposal.height ?? tallest ?? .zero
    guard let width = proposal.width else {
      let ideal = Self.idealWidth(sides: sideWidths(subviews), middle: middleWidth(subviews), gap: gap)
      return CGSize(width: ideal, height: height)
    }
    return CGSize(width: width, height: height)
  }

  /// Places the status at the leading edge, the tabs centred, or clear of the buttons, and the buttons at the
  /// trailing edge.
  ///
  /// - Parameters:
  ///   - bounds: The row.
  ///   - proposal: The size offered.
  ///   - subviews: The status, the tabs and the buttons.
  ///   - cache: Unused.
  func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
    guard subviews.count == 3 else { return }
    let middle = middleWidth(subviews)
    let buttons = subviews[2].sizeThatFits(.unspecified).width
    let tabsMinX = Self.tabsMinX(in: bounds, middle: middle, buttons: buttons, gap: gap)
    subviews[0].place(
      at: CGPoint(x: bounds.minX, y: bounds.midY),
      anchor: .leading,
      proposal: ProposedViewSize(width: max(.zero, tabsMinX - gap - bounds.minX), height: bounds.height)
    )
    subviews[1].place(
      at: CGPoint(x: tabsMinX, y: bounds.midY),
      anchor: .leading,
      proposal: .unspecified
    )
    subviews[2].place(
      at: CGPoint(x: bounds.maxX, y: bounds.midY),
      anchor: .trailing,
      proposal: .unspecified
    )
  }

  /// The ideal widths of the status and the buttons.
  ///
  /// - Parameter subviews: The status, the tabs and the buttons.
  /// - Returns: The two sides' widths.
  private func sideWidths(_ subviews: Subviews) -> [CGFloat] {
    guard subviews.count == 3 else { return [] }
    return [subviews[0], subviews[2]].map { $0.sizeThatFits(.unspecified).width }
  }

  /// The tabs' ideal width.
  ///
  /// - Parameter subviews: The status, the tabs and the buttons.
  /// - Returns: The width.
  private func middleWidth(_ subviews: Subviews) -> CGFloat {
    guard subviews.count == 3 else { return .zero }
    return subviews[1].sizeThatFits(.unspecified).width
  }

  /// The width the row needs: the tabs, a gap each side, and twice the wider side, so the tabs stay centred.
  ///
  /// - Parameters:
  ///   - sides: The status's and the buttons' ideal widths.
  ///   - middle: The tabs' ideal width.
  ///   - gap: Between a side and the tabs.
  /// - Returns: The width.
  static func idealWidth(sides: [CGFloat], middle: CGFloat, gap: CGFloat) -> CGFloat {
    (sides.max() ?? .zero) * 2 + middle + gap * 2
  }

  /// Where the tabs start: centred in the row, unless the buttons would reach them, then just clear of the buttons.
  ///
  /// - Parameters:
  ///   - bounds: The row.
  ///   - middle: The tabs' width.
  ///   - buttons: The buttons' width.
  ///   - gap: Between a side and the tabs.
  /// - Returns: The tabs' leading edge.
  static func tabsMinX(in bounds: CGRect, middle: CGFloat, buttons: CGFloat, gap: CGFloat) -> CGFloat {
    min(bounds.midX - middle / 2, bounds.maxX - buttons - gap - middle)
  }
}
