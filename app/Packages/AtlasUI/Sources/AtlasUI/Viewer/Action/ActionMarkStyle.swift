import CoreGraphics

/// How large a mark draws: whole over a large picture, with its bubbles, keys and tags; or compact over a small one,
/// its ring, dot, pill and path alone.
enum ActionMarkStyle: Equatable, Sendable {
  /// Over a large picture.
  case full
  /// Over a small one, such as a step's.
  case compact

  /// The ring's width and height, a fixed size centred on the spot, where no element's frame is known.
  var ring: CGFloat {
    self == .full ? ViewerLayout.markRing : ViewerLayout.markRingCompact
  }

  /// The corner of the ring around an element.
  var outlineRadius: CGFloat {
    self == .full ? ViewerLayout.markOutlineRadius : ViewerLayout.markOutlineRadiusCompact
  }

  /// The dot where the pointer landed.
  var dot: CGFloat {
    self == .full ? ViewerLayout.markDot : ViewerLayout.markDotCompact
  }

  /// The ring's and the path's line.
  var line: CGFloat {
    self == .full ? Stroke.thick : Stroke.emphasis
  }

  /// The soft glow around a ring and a dot.
  var glow: CGFloat {
    self == .full ? ViewerLayout.markGlow : ViewerLayout.markGlowCompact
  }

  /// The scroll pill's width; its height is ``ViewerLayout/markPillLength`` times it.
  var pill: CGFloat {
    self == .full ? ViewerLayout.markPill : ViewerLayout.markPillCompact
  }
}
