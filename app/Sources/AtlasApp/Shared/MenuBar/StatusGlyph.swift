import AppKit
import AtlasStatus
import AtlasUI

/// The menu bar item's look: Atlas's orbit, a flat planet with a ring passing in front of it, in a box as wide as the
/// item (``StatusItem/Layout/width``) for every state, so the items to its left never move. The orbit is always a
/// template the menu bar inks in its own colour, so it reads on any menu bar, tinted or under Reduce Transparency; a
/// state tells itself apart by a small badge at the orbit's lower right, and two quiet states dim the orbit too:
///
/// | State | Orbit | Badge |
/// |---|---|---|
/// | Idle | whole | none |
/// | Working | whole | a blue disc that breathes |
/// | You have control | whole | a steady blue disc |
/// | Needs you | whole | an orange disc |
/// | Stopped | whole | an orange rounded square |
/// | Not ready | whole | an orange ring |
/// | In use elsewhere | whole | a grey disc |
/// | Logged out | dimmed | a grey ring |
/// | Connecting | dimmed | none |
///
/// ``look(for:)`` decides everything and the drawing only follows it: ``image(for:)`` draws the orbit with a 1-point
/// knockout round the badge's place, and ``StatusBadge`` draws the badge itself as a layer in that hole, so the gap
/// round it is the menu bar's own colour in light and dark, as a Dock badge stands clear of its icon.
///
/// Its numbers (`Layout`) are the glyph's drawing geometry, sized to the menu bar, not design tokens, so they stay
/// here as they are.
enum StatusGlyph {
  /// How the orbit is inked.
  enum Tint: Equatable {
    /// A template image, which the menu bar inks in its own label colour.
    case template
    /// The template, faded to ``StatusGlyph/dimAlpha`` by the status button's layer: present, but not ready for
    /// agents.
    case dimmed
  }

  /// The mark at the orbit's lower right.
  enum Badge: Equatable {
    /// No badge.
    case none
    /// A blue disc: an agent works, or the person holds control.
    case accentDot
    /// An orange disc: the person is needed.
    case attentionDot
    /// An orange rounded square: agents are stopped.
    case attentionSquare
    /// An orange ring: the helper isn't ready.
    case attentionRing
    /// A grey disc: another viewer holds control.
    case quietDot
    /// A grey ring: the helper is logged out.
    case quietRing
  }

  /// Everything the item shows for a state.
  struct Look: Equatable {
    /// How the orbit is inked.
    let tint: Tint
    /// The badge, if any.
    let badge: Badge
    /// Whether the badge breathes, fading 100 → 50% and back, while an action runs.
    let breathes: Bool
  }

  /// 0.42: the dimmed template's alpha.
  static let dimAlpha: CGFloat = 0.42

  /// How opaque the orbit shows for a tint. The image is always drawn whole, and the dim is the opacity of the layer
  /// that holds it, so it can fade.
  ///
  /// - Parameter tint: The tint.
  /// - Returns: ``dimAlpha`` when dimmed, otherwise 1.
  static func opacity(of tint: Tint) -> CGFloat {
    tint == .dimmed ? dimAlpha : Layout.whole
  }

  /// The look for a state of the helper, as the table above sets it.
  ///
  /// - Parameter mode: The helper's mode, as the person reads it.
  /// - Returns: Its look.
  static func look(for mode: Mode.Kind) -> Look {
    switch mode {
    case .idle: Look(tint: .template, badge: .none, breathes: false)
    case .working: Look(tint: .template, badge: .accentDot, breathes: true)
    case .control: Look(tint: .template, badge: .accentDot, breathes: false)
    case .needsYou: Look(tint: .template, badge: .attentionDot, breathes: false)
    case .stopped: Look(tint: .template, badge: .attentionSquare, breathes: false)
    case .notReady: Look(tint: .template, badge: .attentionRing, breathes: false)
    case .inUseElsewhere: Look(tint: .template, badge: .quietDot, breathes: false)
    case .loggedOut: Look(tint: .dimmed, badge: .quietRing, breathes: false)
    case .connecting: Look(tint: .dimmed, badge: .none, breathes: false)
    }
  }

  /// The orbit for a look, centred in the item's square and drawn whole (a dimmed look fades it with
  /// ``opacity(of:)``), with the badge's ring knocked out of it.
  ///
  /// - Parameter look: The look.
  /// - Returns: The image for the status item's button, always a template.
  static func image(for look: Look) -> NSImage {
    let side = StatusItem.Layout.width
    let image = NSImage(size: NSSize(width: side, height: side), flipped: true) { _ in
      guard let context = NSGraphicsContext.current?.cgContext else { return false }
      NSColor.black
        .set()
      context.saveGState()
      // Drawn in one layer, so the knockout cuts the orbit alone.
      context.beginTransparencyLayer(auxiliaryInfo: nil)
      Orbit(
        center: CGPoint(x: side / 2, y: side / 2),
        width: Layout.orbit,
        pixel: 1 / pixelScale(),
        least: Layout.orbitLeast
      )
      .draw(in: context)
      if let ring = ring(look.badge) {
        context.setBlendMode(.destinationOut)
        context.addPath(ring)
        context.fillPath()
      }
      context.endTransparencyLayer()
      context.restoreGState()
      return true
    }
    image.isTemplate = true
    return image
  }

  /// Where a badge sits in the item's square, top down: at the orbit's lower right, on whole points, so it lands on
  /// whole pixels at one pixel a point too. Every badge shares the box.
  ///
  /// - Parameter badge: The badge.
  /// - Returns: Its box, or nil for none.
  static func frame(of badge: Badge) -> CGRect? {
    badge == .none ? nil : Layout.badge
  }

  /// A badge's outline in a box of its own size: a rounded square either way, its corners half its side for the disc
  /// and the ring, so every badge shares one shape of path and one can turn into another. A ring's outline runs down
  /// the middle of its stroke, half the stroke in from the box, so the stroke fills the box exactly.
  ///
  /// - Parameters:
  ///   - badge: The badge.
  ///   - box: The badge's box.
  /// - Returns: A disc or a ring, or a rounded square for Stopped.
  static func outline(of badge: Badge, in box: CGRect) -> CGPath {
    let shape = isHollow(badge) ? box.insetBy(dx: ringStroke / 2, dy: ringStroke / 2) : box
    let corner = badge == .attentionSquare ? Layout.squareCorner : min(shape.width, shape.height) / 2
    return CGPath(roundedRect: shape, cornerWidth: corner, cornerHeight: corner, transform: nil)
  }

  /// Whether a badge is a ring, stroked rather than filled.
  ///
  /// - Parameter badge: The badge.
  /// - Returns: True for the rings.
  static func isHollow(_ badge: Badge) -> Bool {
    badge == .attentionRing || badge == .quietRing
  }

  /// 1.5: a ring badge's stroke.
  static let ringStroke: CGFloat = 1.5

  /// A badge's colour.
  ///
  /// - Parameter badge: The badge.
  /// - Returns: The system's blue, orange or grey, or nil for none.
  static func colour(of badge: Badge) -> NSColor? {
    switch badge {
    case .none: nil
    case .accentDot: .systemBlue
    case .attentionDot, .attentionSquare, .attentionRing: .systemOrange
    case .quietDot, .quietRing: .systemGray
    }
  }

  /// The badge's outline grown by its ring, which is cut out of the orbit.
  ///
  /// - Parameter badge: The badge.
  /// - Returns: The ring's outer outline in the item's square, or nil for none.
  private static func ring(_ badge: Badge) -> CGPath? {
    guard let frame = frame(of: badge) else { return nil }
    let outer = frame.insetBy(dx: -Layout.ring, dy: -Layout.ring)
    return badge == .attentionSquare
      ? CGPath(
        roundedRect: outer,
        cornerWidth: Layout.squareCorner + Layout.ring,
        cornerHeight: Layout.squareCorner + Layout.ring,
        transform: nil
      )
      : CGPath(ellipseIn: outer, transform: nil)
  }

  /// Device pixels per point of the context being drawn into, so lines stay
  /// crisp on both standard and Retina displays.
  ///
  /// - Returns: `Layout.fallbackScale` when no context is current.
  private static func pixelScale() -> CGFloat {
    guard let transform = NSGraphicsContext.current?.cgContext.userSpaceToDeviceSpaceTransform else {
      return Layout.fallbackScale
    }
    let scale = (transform.a * transform.a + transform.b * transform.b)
      .squareRoot()
      .rounded()
    return max(1, scale)
  }
}

/// The glyph's drawing geometry, in points of the menu bar's 22-point item.
private enum Layout {
  /// 17.9: the orbit's width, the ring's outer reach, in an 18 × 10.3 box.
  static let orbit: CGFloat = 17.9
  /// 1: the thinnest the orbit's line draws.
  static let orbitLeast: CGFloat = 1
  /// 15, 13, 7 × 7: the badge's box in the item's square, top down, at the orbit's lower right: its centre 1.5 in from
  /// the orbit's right end, its bottom 2 up from the item's bottom edge. A 7-point disc, ring or square, on whole
  /// pixels at one pixel a point.
  static let badge = CGRect(x: 15, y: 13, width: 7, height: 7)
  /// 2: the square's corners.
  static let squareCorner: CGFloat = 2
  /// 1: the gap knocked out of the orbit around the badge.
  static let ring: CGFloat = 1
  /// 1: an orbit shown whole.
  static let whole: CGFloat = 1
  /// 2: the pixels a point is taken to have when no context is current, a Retina display's.
  static let fallbackScale: CGFloat = 2
}
