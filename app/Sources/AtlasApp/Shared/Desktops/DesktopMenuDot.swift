import AppKit
import AtlasUI

/// A desktop's dot as its item in the desktops' menu draws it, in its state's tone and mark: a disc, a smaller disc
/// while nothing can happen yet, a rounded square once agents are stopped, or a ring while waiting its turn. Drawn
/// when the menu draws, so its colour follows the menu's appearance; never a template, so the menu keeps its colour.
enum DesktopMenuDot {
  /// 12: the image's side, the room a menu item's image has beside its title.
  static let side: CGFloat = 12
  /// 8: a dot's diameter, as the panel's small status dots are.
  static let dot: CGFloat = 8
  /// 6: the smaller dot's diameter.
  static let small: CGFloat = 6
  /// 2: the square's corner radius.
  static let squareCorner: CGFloat = 2
  /// 1.5: the ring's stroke, as the menu bar glyph's ring.
  static let ringStroke: CGFloat = 1.5

  /// The dot of a state in `tone` and `mark`.
  ///
  /// - Parameters:
  ///   - tone: The state's tone.
  ///   - mark: The state's mark.
  /// - Returns: The image.
  static func image(tone: StatusTone, mark: StatusMark) -> NSImage {
    let image = NSImage(size: NSSize(width: side, height: side), flipped: false) { bounds in
      let colour = Self.colour(of: tone)
      let diameter = mark == .small ? small : dot
      let box = NSRect(
        x: bounds.midX - diameter / 2,
        y: bounds.midY - diameter / 2,
        width: diameter,
        height: diameter
      )
      switch mark {
      case .square:
        colour.setFill()
        NSBezierPath(roundedRect: box, xRadius: squareCorner, yRadius: squareCorner)
          .fill()
      case .ring:
        colour.setStroke()
        let ring = NSBezierPath(ovalIn: box.insetBy(dx: ringStroke / 2, dy: ringStroke / 2))
        ring.lineWidth = ringStroke
        ring.stroke()
      case .dot, .pulsing, .small:
        colour.setFill()
        NSBezierPath(ovalIn: box)
          .fill()
      }
      return true
    }
    image.isTemplate = false
    return image
  }

  /// A tone's paint, as the windows' dots have it: green when ready, blue while working or in control, orange when it
  /// needs the person, grey otherwise.
  ///
  /// - Parameter tone: The tone.
  /// - Returns: The colour.
  static func colour(of tone: StatusTone) -> NSColor {
    switch tone {
    case .ready: .systemGreen
    case .active, .link: .systemBlue
    case .attention: .systemOrange
    case .quiet: .tertiaryLabelColor
    }
  }
}
