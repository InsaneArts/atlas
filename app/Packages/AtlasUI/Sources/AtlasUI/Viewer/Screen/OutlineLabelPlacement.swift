import SwiftUI

/// Places a window outline's label over the picture (v3 V14): just above the outline at its left edge, or, near an
/// edge where that would leave the picture, inside the outline at its top right, as for a window docked at the right
/// with a long name. Never wider than the picture less an inset each side, and never past its edges.
struct OutlineLabelPlacement: Layout {
  /// Where the window lies, in the picture.
  let outline: CGRect

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

  /// Places the label where ``frame(label:outline:in:)`` puts it.
  ///
  /// - Parameters:
  ///   - bounds: The picture's bounds.
  ///   - proposal: The space offered.
  ///   - subviews: The label.
  ///   - cache: Unused.
  func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
    for subview in subviews {
      let placed = Self.frame(label: subview.sizeThatFits(.unspecified), outline: outline, in: bounds.size)
      subview.place(
        at: CGPoint(x: bounds.minX + placed.minX, y: bounds.minY + placed.minY),
        proposal: ProposedViewSize(placed.size)
      )
    }
  }

  /// Where the label goes: above the outline, its bottom ``Metrics/gap`` over the outline's top, at the outline's left
  /// edge; but inside the outline, ``Metrics/inside`` from its top and right edges, when above would run past the
  /// picture's top or right edge. Either way it is no wider than the picture less ``Metrics/inset`` each side, and kept
  /// that far inside the picture.
  ///
  /// - Parameters:
  ///   - label: The label's own size.
  ///   - outline: Where the window lies, in the picture.
  ///   - picture: The picture's size.
  /// - Returns: The label's frame, in the picture.
  nonisolated static func frame(label: CGSize, outline: CGRect, in picture: CGSize) -> CGRect {
    let width = min(label.width, max(0, picture.width - Metrics.inset * 2))
    let rightmost = picture.width - Metrics.inset - width
    let above = CGPoint(x: max(Metrics.inset, outline.minX), y: outline.minY - Metrics.gap - label.height)
    let origin =
      above.y >= Metrics.inset && above.x <= rightmost
      ? above
      : CGPoint(
        x: max(Metrics.inset, min(outline.maxX - Metrics.inside - width, rightmost)),
        y: max(Metrics.inset, outline.minY + Metrics.inside)
      )
    return CGRect(origin: origin, size: CGSize(width: width, height: label.height))
  }
}

extension OutlineLabelPlacement {
  /// The label's own measures (v3 `.outline .olabel`).
  enum Metrics {
    /// 10: between the label's bottom and the outline's top, when it sits above.
    static let gap = Spacing.sPlus
    /// 8: how far inside the outline's top and right edges the label sits, near an edge.
    static let inside = Spacing.s
    /// 8: how far inside the picture's edges the label always keeps.
    static let inset = Spacing.s
  }
}

#if DEBUG
  #Preview("Near an edge") {
    ModifierSamples(.outlineLabel)
  }
#endif
