import SwiftUI

/// Places a window outline's label over the picture: at the outline's left edge and above it, shortened to the
/// picture's width less an inset each side, and moved left so it never runs past the right edge, as for a window
/// docked at the right with a long name.
struct OutlineLabelPlacement: Layout {
  /// Where the label's top-left corner goes when it fits, in the picture.
  let anchor: CGPoint

  /// Takes the whole space offered, the picture's.
  ///
  /// - Parameters:
  ///   - proposal: The space offered.
  ///   - subviews: The label.
  ///   - cache: Unused.
  /// - Returns: The space offered.
  func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
    proposal.replacingUnspecifiedDimensions()
  }

  /// Places the label at its own width, no wider than the picture allows, kept inside the picture.
  ///
  /// - Parameters:
  ///   - bounds: The picture's bounds.
  ///   - proposal: The space offered.
  ///   - subviews: The label.
  ///   - cache: Unused.
  func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
    for subview in subviews {
      let natural = subview.sizeThatFits(.unspecified)
      let width = min(natural.width, max(0, bounds.width - Spacing.m * 2))
      let x = max(Spacing.m, min(anchor.x, bounds.width - width - Spacing.m))
      subview.place(
        at: CGPoint(x: bounds.minX + x, y: bounds.minY + anchor.y),
        proposal: ProposedViewSize(width: width, height: natural.height)
      )
    }
  }
}
