import SwiftUI

/// Offers its one view a fixed height, whatever its parent offers, and takes the view's own size. A `ViewThatFits`
/// inside then chooses by that height alone, even in a scroll view or a stack that offers no height at all, and a
/// shorter choice leaves no gap.
struct HeightBudget: Layout {
  /// The height offered.
  let height: CGFloat

  /// The view's own size at the parent's width and the budget's height.
  ///
  /// - Parameters:
  ///   - proposal: What the parent offers; only its width is passed on.
  ///   - subviews: The one view.
  ///   - cache: Unused.
  /// - Returns: The view's size.
  func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
    guard let view = subviews.first else { return .zero }
    return view.sizeThatFits(ProposedViewSize(width: proposal.width, height: height))
  }

  /// Places the view at the top, offered the same height.
  ///
  /// - Parameters:
  ///   - bounds: Where the view goes.
  ///   - proposal: What the parent offered.
  ///   - subviews: The one view.
  ///   - cache: Unused.
  func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
    subviews.first?
      .place(at: bounds.origin, anchor: .topLeading, proposal: ProposedViewSize(width: bounds.width, height: height))
  }
}
