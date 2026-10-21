import SwiftUI

/// Views laid in rows, left to right, starting a new row when the next one would not fit: chips that wrap instead of
/// running past their card. A view wider than a row takes a row of its own, at the row's width.
struct WrappingStack: Layout {
  /// The gap between views in a row, and between rows.
  var spacing: CGFloat

  /// The rows' size within the width offered.
  ///
  /// - Parameters:
  ///   - proposal: The size offered.
  ///   - subviews: The views to lay out.
  ///   - cache: Unused.
  /// - Returns: The widest row's width and the rows' height.
  func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
    let frames = frames(subviews, width: proposal.width ?? .infinity)
    let width =
      frames.map(\.maxX)
      .max() ?? .zero
    let height =
      frames.map(\.maxY)
      .max() ?? .zero
    return CGSize(width: width, height: height)
  }

  /// Places each view in its row.
  ///
  /// - Parameters:
  ///   - bounds: Where the rows go.
  ///   - proposal: The size offered.
  ///   - subviews: The views to lay out.
  ///   - cache: Unused.
  func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
    for (subview, frame) in zip(subviews, frames(subviews, width: bounds.width)) {
      subview.place(
        at: CGPoint(x: bounds.minX + frame.minX, y: bounds.minY + frame.minY),
        proposal: ProposedViewSize(frame.size)
      )
    }
  }

  /// Each view's frame, row by row within `width`.
  ///
  /// - Parameters:
  ///   - subviews: The views to lay out.
  ///   - width: The rows' width.
  /// - Returns: One frame a view, in order.
  private func frames(_ subviews: Subviews, width: CGFloat) -> [CGRect] {
    var frames: [CGRect] = []
    var origin = CGPoint.zero
    var rowHeight: CGFloat = .zero
    for subview in subviews {
      var size = subview.sizeThatFits(.unspecified)
      size.width = min(size.width, width)
      if origin.x > .zero, origin.x + size.width > width {
        origin = CGPoint(x: .zero, y: origin.y + rowHeight + spacing)
        rowHeight = .zero
      }
      frames.append(CGRect(origin: origin, size: size))
      origin.x += size.width + spacing
      rowHeight = max(rowHeight, size.height)
    }
    return frames
  }
}

#if DEBUG
  #Preview("Wrapping chips") {
    WrappingStack(spacing: Spacing.s) {
      ForEach(PreviewStates.onboardingAgentsManyProjects) { project in
        ProjectChip(project: project) {}
      }
    }
    .frame(width: SampleWidth.column)
    .sampleSurface()
  }
#endif
