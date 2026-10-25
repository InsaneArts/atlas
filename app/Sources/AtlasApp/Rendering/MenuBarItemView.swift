import AppKit

/// The menu bar item as the renders draw it: the status button's image, a template inked as the menu bar inks one, at
/// the opacity the button's layer gives it, then the live badge's layer over it. `cacheDisplay` draws views, not the
/// layers added to them, and an image view draws at one pixel a point when it has no window, so both are drawn here,
/// at the bitmap's own scale.
final class MenuBarItemView: NSView {
  /// The status button's image.
  private let image: NSImage
  /// The opacity the button's layer shows the image at: less than whole for a dimmed orbit.
  private let opacity: CGFloat
  /// What a template is inked in: the menu bar's label colour.
  private let ink: NSColor
  /// The badge's layer, laid out in this view's coordinates, bottom up.
  private let badge: CALayer

  /// The item at `frame`.
  ///
  /// - Parameters:
  ///   - image: The status button's image, drawn centred.
  ///   - opacity: The opacity the button's layer shows it at.
  ///   - ink: What a template is inked in.
  ///   - badge: The badge's layer.
  ///   - frame: The view's frame, the item's square.
  init(image: NSImage, opacity: CGFloat, ink: NSColor, badge: CALayer, frame: NSRect) {
    self.image = image
    self.opacity = opacity
    self.ink = ink
    self.badge = badge
    super.init(frame: frame)
  }

  /// Not made from an archive.
  ///
  /// - Parameter coder: Unused.
  required init?(coder: NSCoder) { nil }

  /// Draws the image, inked when it is a template and faded as a whole, then the badge at its frame unless it is
  /// hidden.
  ///
  /// - Parameter dirtyRect: The area to draw.
  override func draw(_ dirtyRect: NSRect) {
    guard let context = NSGraphicsContext.current?.cgContext else { return }
    let box = NSRect(
      x: (bounds.width - image.size.width) / 2,
      y: (bounds.height - image.size.height) / 2,
      width: image.size.width,
      height: image.size.height
    )
    context.saveGState()
    context.setAlpha(opacity)
    context.beginTransparencyLayer(auxiliaryInfo: nil)
    image.draw(in: box)
    if image.isTemplate {
      ink.set()
      box.fill(using: .sourceAtop)
    }
    context.endTransparencyLayer()
    context.restoreGState()
    guard !badge.isHidden else { return }
    context.saveGState()
    context.translateBy(x: badge.frame.minX, y: badge.frame.minY)
    badge.render(in: context)
    context.restoreGState()
  }
}
