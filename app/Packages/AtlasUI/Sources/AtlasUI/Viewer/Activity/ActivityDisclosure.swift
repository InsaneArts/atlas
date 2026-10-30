import SwiftUI

/// A moment's header and the rows under it, laid out one under another at their own heights, the whole standing as
/// tall as the header and `share` of the rows, so as the share moves on one spring the moment grows or shrinks
/// continuously, its rows standing still in place while its edge reveals or covers them, and everything below moves
/// with that edge. The share is the layout's animatable value, so a second press mid-way turns it back from where it
/// is.
struct ActivityDisclosure: Layout {
  /// How much of the rows shows, from 0 (folded) to 1 (open).
  var share: CGFloat
  /// The room kept under the rows while any show, which grows and shrinks with them.
  var below: CGFloat

  /// A row starts fading only after its entire text and callout fit above the disclosure edge. The same geometry
  /// hides it before folding can cut its letters, and reversing the spring reverses the fade without queued work.
  static func opacity(row: CGRect, bounds: CGRect?) -> Double {
    guard let bounds else { return 1 }
    return Double(min(max((bounds.maxY - row.maxY) / Spacing.s, .zero), 1))
  }

  /// The share, which an animation moves frame by frame.
  var animatableData: CGFloat {
    get { share }
    set { share = newValue }
  }

  /// The width offered, and the header's height with `share` of the rows' and the room under them.
  ///
  /// - Parameters:
  ///   - proposal: The room offered.
  ///   - subviews: The header, then the rows.
  ///   - cache: Unused.
  /// - Returns: The size.
  func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
    let heights = Self.heights(of: subviews, width: proposal.width)
    let header = heights.first ?? .zero
    let rows = heights.dropFirst()
      .reduce(.zero, +)
    let room = rows > .zero ? rows + below : .zero
    let width = proposal.width ?? Self.widest(subviews)
    return CGSize(width: width, height: header + room * min(max(share, .zero), 1))
  }

  /// The widest of the subviews at their own size, for a proposal that names no width.
  ///
  /// - Parameter subviews: The header and the rows.
  /// - Returns: The width.
  private static func widest(_ subviews: Subviews) -> CGFloat {
    subviews
      .map { $0.sizeThatFits(.unspecified).width }
      .max() ?? .zero
  }

  /// Places the header and then each row at its own height, from the top, so no row moves as the share does.
  ///
  /// - Parameters:
  ///   - bounds: Where the moment stands.
  ///   - proposal: The room offered.
  ///   - subviews: The header, then the rows.
  ///   - cache: Unused.
  func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
    let heights = Self.heights(of: subviews, width: bounds.width)
    var top = bounds.minY
    for (subview, height) in zip(subviews, heights) {
      subview.place(
        at: CGPoint(x: bounds.minX, y: top),
        anchor: .topLeading,
        proposal: ProposedViewSize(width: bounds.width, height: height)
      )
      top += height
    }
  }

  /// Each child's own height at `width`.
  ///
  /// - Parameters:
  ///   - subviews: The children.
  ///   - width: The width they are laid out at.
  /// - Returns: Their heights, in order.
  private static func heights(of subviews: Subviews, width: CGFloat?) -> [CGFloat] {
    subviews.map { $0.sizeThatFits(ProposedViewSize(width: width, height: nil)).height }
  }
}
