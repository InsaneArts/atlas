import AppKit
import AtlasKit
import AtlasUI
import SwiftUI

/// The panel's render, `--render PATH`, in release builds too: it draws from the live service, not from samples.
@MainActor
enum PanelRender {
  /// The menu bar strip's height above the panel: 28, a menu bar's on a laptop without a notch.
  static let barHeight: CGFloat = 28
  /// The gap between the strip and the panel, as the popover leaves it: 8.
  static let gap: CGFloat = 8
  /// How far in from the strip's right edge the glyph sits: 40, about where the system puts a new status item.
  static let glyphInset: CGFloat = 40

  /// Draws the panel and its menu bar glyph, as a person would see them, to a PNG at `path`, from what the viewer
  /// service reports now. The panel is the presenter's own output for the live status, changed only by `overrides`,
  /// so the render shows exactly what the shipped panel shows.
  ///
  /// - Parameters:
  ///   - service: The viewer service to read.
  ///   - overrides: What the design check changes, from `ATLAS_RENDER_*`.
  ///   - path: Where to write the PNG.
  ///   - name: The appearance of the menu bar strip around the panel.
  /// - Returns: Whether the PNG was written.
  static func draw(
    service: any ViewerService,
    overrides: PanelRenderOverrides,
    to path: String,
    appearance name: NSAppearance.Name
  ) async -> Bool {
    async let reading = try? service.status()
    async let picture = try? service.frame()
    let status = await reading ?? ViewerStatus()
    let frame = await picture
    let snapshot = overrides.applied(
      to: PanelSnapshot(
        status: status,
        picture: frame.flatMap { DesktopPicture(frame: $0) },
        helper: status.overview.account
      )
    )
    let content = NSHostingView(rootView: PanelView(state: PanelPresenter.state(snapshot)) { _ in })
    let size = content.fittingSize
    let canvas = NSView(frame: NSRect(x: 0, y: 0, width: size.width, height: size.height + barHeight + gap))
    let background = NSBox(frame: canvas.bounds)
    background.boxType = .custom
    background.borderWidth = 0
    background.fillColor = .windowBackgroundColor
    canvas.addSubview(background)
    let strip = NSBox(frame: NSRect(x: 0, y: size.height + gap, width: size.width, height: barHeight))
    strip.boxType = .custom
    strip.borderWidth = 0
    strip.fillColor = .underPageBackgroundColor
    canvas.addSubview(strip)
    // The glyph at its own size, centred in the strip as the menu bar does.
    let image = StatusGlyph.image(for: Mode.of(snapshot.status.overview.mode))
    let glyph = NSImageView(image: image)
    glyph.imageScaling = .scaleNone
    glyph.frame = NSRect(
      x: (size.width - glyphInset).rounded(),
      y: (size.height + gap + (barHeight - image.size.height) / 2).rounded(.down),
      width: image.size.width,
      height: image.size.height
    )
    canvas.addSubview(glyph)
    content.frame = NSRect(origin: .zero, size: size)
    canvas.addSubview(content)
    guard let appearance = NSAppearance(named: name), let data = PNG.data(of: canvas, appearance: appearance) else {
      return false
    }
    return (try? data.write(to: URL(fileURLWithPath: path))) != nil
  }
}
