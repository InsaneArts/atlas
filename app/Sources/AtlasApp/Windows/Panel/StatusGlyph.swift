import AppKit
import AtlasKit
import AtlasUI

/// The menu bar glyph: Atlas's orbit, a flat planet with a ring passing in front of it, then the status's mark
/// beside it, as the system's own indicators carry one: a blue dot while an agent works or the person holds
/// control, an orange dot when the person is needed, an orange square once stopped, and a small grey dot while the
/// helper can't be used. Idle carries none. Only the plain orbit is a template; with a mark it draws in the menu
/// bar's label colour instead.
///
/// Its numbers (`Layout`) are the glyph's drawing geometry, sized to the menu bar (mac.py's panel board), not design
/// tokens, so they stay here as they are.
enum StatusGlyph {
  /// The mark beside the orbit.
  private enum Mark: Equatable {
    /// A dot that glows.
    case glowing(NSColor)
    /// The stopped square.
    case square
    /// The small grey dot.
    case quiet
  }

  /// The glyph for a mode.
  ///
  /// - Parameter mode: The helper's mode, as the person reads it.
  /// - Returns: The image for the status item's button, a template unless it carries a mark.
  static func image(for mode: Mode) -> NSImage {
    let mark = mark(for: mode)
    let size = NSSize(width: mark == nil ? Layout.box.width : Layout.boxWithMark, height: Layout.box.height)
    let image = NSImage(size: size, flipped: true) { _ in
      let pixel = 1 / Self.pixelScale()
      // Drawn opaque in one layer, then faded as a whole by the label colour's own alpha, so overlapping strokes
      // never darken.
      let ink = mark == nil ? NSColor.black : (NSColor.labelColor.usingColorSpace(.sRGB) ?? .black)
      ink
        .withAlphaComponent(1)
        .set()
      guard let context = NSGraphicsContext.current?.cgContext else { return false }
      context.saveGState()
      context.setAlpha(ink.alphaComponent)
      context.beginTransparencyLayer(auxiliaryInfo: nil)
      // The ring a little bolder than the drawing's, so it and the gap beside it stay clear on Retina displays.
      let center = CGPoint(x: Layout.box.width / 2, y: Layout.box.height / 2)
      Orbit(center: center, width: Layout.orbit, pixel: pixel, least: Layout.orbitLeast)
        .draw(in: context)
      context.endTransparencyLayer()
      context.restoreGState()
      if let mark { draw(mark, in: context) }
      return true
    }
    image.isTemplate = mark == nil
    image.accessibilityDescription = PanelWords.statusItem(mode.label)
    return image
  }

  /// The mark a mode carries, per the status table's menu bar column.
  ///
  /// - Parameter mode: The helper's mode.
  /// - Returns: The mark, or nil for none.
  private static func mark(for mode: Mode) -> Mark? {
    switch mode.kind {
    case .idle, .inUseElsewhere: nil
    case .working, .control: .glowing(.systemBlue)
    case .needsYou: .glowing(.systemOrange)
    case .stopped: .square
    case .notReady, .loggedOut, .connecting: .quiet
    }
  }

  /// Draws a mark beside the orbit, centred on the box's height.
  ///
  /// - Parameters:
  ///   - mark: The mark.
  ///   - context: The glyph's context.
  private static func draw(_ mark: Mark, in context: CGContext) {
    switch mark {
    case .glowing(let colour):
      context.saveGState()
      context.setShadow(offset: .zero, blur: Layout.glow, color: colour.cgColor)
      colour.setFill()
      NSBezierPath(ovalIn: rect(side: Layout.dot))
        .fill()
      context.restoreGState()
    case .square:
      NSColor.systemOrange
        .setFill()
      NSBezierPath(roundedRect: rect(side: Layout.square), xRadius: Layout.squareCorner, yRadius: Layout.squareCorner)
        .fill()
    case .quiet:
      NSColor.systemGray
        .setFill()
      NSBezierPath(ovalIn: rect(side: Layout.quietDot))
        .fill()
    }
  }

  /// A mark's square box, after the orbit and its gap, centred on the box's height.
  ///
  /// - Parameter side: The mark's side.
  /// - Returns: Its box.
  private static func rect(side: CGFloat) -> NSRect {
    NSRect(x: Layout.markX, y: (Layout.box.height - side) / 2, width: side, height: side)
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

/// The glyph's drawing geometry, in points of the menu bar.
private enum Layout {
  /// 18 by 16: the orbit's box.
  static let box = NSSize(width: 18, height: 16)
  /// 17.4: the orbit's width, the ring's outer reach.
  static let orbit: CGFloat = 17.4
  /// 1: the thinnest the orbit's line draws.
  static let orbitLeast: CGFloat = 1
  /// 22: where a mark starts, 4 past the orbit's box, the board's gap.
  static let markX: CGFloat = 22
  /// 31: the box's width with a mark, room for the 6 square and a dot's glow after it.
  static let boxWithMark: CGFloat = 31
  /// 5: a glowing dot.
  static let dot: CGFloat = 5
  /// 3: the dot's glow, the board's 8-point CSS blur kept inside the menu bar's 16-point box.
  static let glow: CGFloat = 3
  /// 6: the stopped square.
  static let square: CGFloat = 6
  /// 2: the square's corners, softened as the system's stop buttons are.
  static let squareCorner: CGFloat = 2
  /// 4: the small grey dot of a helper that can't be used.
  static let quietDot: CGFloat = 4
  /// 2: the pixels a point is taken to have when no context is current, a Retina display's.
  static let fallbackScale: CGFloat = 2
}
