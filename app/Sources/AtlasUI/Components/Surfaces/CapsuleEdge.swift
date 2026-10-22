import SwiftUI

/// A capsule drawn as a rounded rectangle whose corners are half its height, round as the system's are. A stroked
/// `Capsule` leaves a hairline outside each end where AppKit draws the view into an image, as the design renders do;
/// this draws the same edge without it.
struct CapsuleEdge: InsettableShape {
  /// How far the shape is drawn in from its frame, for `strokeBorder`.
  var inset: CGFloat = 0

  /// The capsule in `rect`, drawn in by ``inset``.
  ///
  /// - Parameter rect: The frame.
  /// - Returns: The outline.
  func path(in rect: CGRect) -> Path {
    let frame = rect.insetBy(dx: inset, dy: inset)
    return RoundedRectangle(cornerRadius: min(frame.width, frame.height) / 2, style: .circular)
      .path(in: frame)
  }

  /// The capsule drawn in by `amount` more.
  ///
  /// - Parameter amount: How much further in.
  /// - Returns: The shape.
  func inset(by amount: CGFloat) -> CapsuleEdge {
    CapsuleEdge(inset: inset + amount)
  }
}
