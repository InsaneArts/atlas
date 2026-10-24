import AppKit
import AtlasService
import AtlasUI
import SwiftUI

/// The panel's render, `--render PATH`, in release builds too: it draws from the live service, not from samples,
/// changed only by ``PanelRenderOverrides``, so every state, and the alert window, can be drawn offline.
enum PanelRender {
  /// The menu bar strip's height above the panel: 28, a menu bar's on a laptop without a notch.
  static let barHeight: CGFloat = 28
  /// How far in from the strip's right edge the glyph sits: 40, about where the system puts a new status item.
  static let glyphInset: CGFloat = 40
  /// 40: the room around the panel's card for its shadow, its 20 fall plus most of its 25 blur.
  static let margin: CGFloat = 40
  /// The modes `ATLAS_RENDER_MODE` names, one for each of the panel's states, as the design check renders them:
  /// Idle, Working, You have control, Needs you, Logged out, Not ready, Stopped, Connecting and In use elsewhere.
  nonisolated static let modes = [
    "observing", "agent", "controlling", "recovering", "loggedOut", "unavailable", "stopped", "connecting",
    "elsewhere",
  ]
  /// Each alert window render's file name with the `ATLAS_RENDER_*` settings that draw it: every alert, one whose
  /// command did not run, and one whose command runs.
  nonisolated static let alerts: [(file: String, environment: [String: String])] = [
    ("alert-recover", ["ATLAS_RENDER_ALERT": "recover", "ATLAS_RENDER_MODE": "recovering"]),
    ("alert-switch", ["ATLAS_RENDER_ALERT": "switch"]),
    ("alert-log-out", ["ATLAS_RENDER_ALERT": "logOut"]),
    ("alert-log-out-anyway", ["ATLAS_RENDER_ALERT": "logOutAnyway"]),
    (
      "alert-recover-refused",
      [
        "ATLAS_RENDER_ALERT": "recover", "ATLAS_RENDER_MODE": "recovering",
        "ATLAS_RENDER_MESSAGE": "The helper is in use. Try Recover again once it's free.",
      ]
    ),
    (
      "alert-recovering",
      ["ATLAS_RENDER_ALERT": "recover", "ATLAS_RENDER_MODE": "recovering", "ATLAS_RENDER_BUSY": "1"]
    ),
  ]

  /// Draws the panel and its menu bar glyph, as a person would see them, to a PNG at `path`, from what the viewer
  /// service reports now, changed only by `overrides`; or, when they name one, the alert window's alert.
  ///
  /// - Parameters:
  ///   - service: The viewer service to read.
  ///   - overrides: What the design check changes, from `ATLAS_RENDER_*`.
  ///   - path: Where to write the PNG.
  ///   - appearance: The appearance of the menu bar strip around the panel.
  /// - Returns: Whether the PNG was written.
  static func draw(
    service: any ViewerService,
    overrides: PanelRenderOverrides,
    to path: String,
    appearance: NSAppearance?
  ) async -> Bool {
    async let reading = try? service.status()
    async let picture = try? service.frame()
    let status = await reading ?? ViewerStatus()
    let frame = await picture
    var live = PanelSnapshot(status: status, picture: frame.flatMap { DesktopPicture(frame: $0) })
    #if DEBUG
      if overrides.showsEverything, live.picture == nil { live.picture = SampleDesktop.picture }
    #endif
    let snapshot = overrides.applied(to: live)
    if let alert = overrides.alertSnapshot(over: snapshot.status.overview) {
      return draw(alert: alert, to: path, appearance: appearance)
    }
    return draw(snapshot, to: path, appearance: appearance)
  }

  /// Draws the panel for `snapshot` on its card, under a strip of menu bar holding its glyph.
  ///
  /// - Parameters:
  ///   - snapshot: What the panel's controller would hold.
  ///   - path: Where to write the PNG.
  ///   - appearance: The appearance of the menu bar strip around the panel.
  /// - Returns: Whether the PNG was written.
  static func draw(_ snapshot: PanelSnapshot, to path: String, appearance: NSAppearance?) -> Bool {
    let content = WindowShell.hostingView(
      ShellRoot {
        PanelView(state: PanelPresenter.state(snapshot)) { _ in }
          .panelCard()
          .padding(margin)
      }
    )
    let size = content.fittingSize
    let canvas = NSView(frame: NSRect(x: 0, y: 0, width: size.width, height: size.height + barHeight))
    canvas.addSubview(fill(canvas.bounds, .windowBackgroundColor))
    let strip = NSRect(x: 0, y: size.height, width: size.width, height: barHeight)
    canvas.addSubview(fill(strip, .underPageBackgroundColor))
    // The glyph at its own size, centred in the strip as the menu bar does.
    let image = StatusGlyph.image(for: PanelPresenter.mode(snapshot))
    let glyph = NSImageView(image: image)
    glyph.imageScaling = .scaleNone
    glyph.frame = NSRect(
      x: (size.width - margin - glyphInset).rounded(),
      y: (size.height + (barHeight - image.size.height) / 2).rounded(.down),
      width: image.size.width,
      height: image.size.height
    )
    canvas.addSubview(glyph)
    content.frame = NSRect(origin: .zero, size: size)
    canvas.addSubview(content)
    return write(canvas, to: path, appearance: appearance)
  }

  /// Draws the alert window's alert, as it stands over whatever is behind it: running, saying why its command did
  /// not run, or as first asked.
  ///
  /// - Parameters:
  ///   - alert: The alert, as the alert window's controller would hold it.
  ///   - path: Where to write the PNG.
  ///   - appearance: The appearance of the ground behind it.
  /// - Returns: Whether the PNG was written.
  static func draw(alert: AlertSnapshot, to path: String, appearance: NSAppearance?) -> Bool {
    let state = AlertPresenter.state(alert)
    let content = WindowShell.hostingView(ShellRoot { AlertView(state: state) { _ in } })
    let size = content.fittingSize
    let canvas = NSView(frame: NSRect(origin: .zero, size: size))
    canvas.addSubview(fill(canvas.bounds, .windowBackgroundColor))
    content.frame = canvas.bounds
    canvas.addSubview(content)
    return write(canvas, to: path, appearance: appearance)
  }

  /// A plain box in one colour.
  ///
  /// - Parameters:
  ///   - frame: Where it sits.
  ///   - colour: Its colour.
  /// - Returns: The box.
  private static func fill(_ frame: NSRect, _ colour: NSColor) -> NSBox {
    let box = NSBox(frame: frame)
    box.boxType = .custom
    box.borderWidth = 0
    box.fillColor = colour
    return box
  }

  /// Writes `canvas` as a PNG at `path`.
  ///
  /// - Parameters:
  ///   - canvas: The view to draw.
  ///   - path: Where to write it.
  ///   - appearance: The appearance to draw it in.
  /// - Returns: Whether the PNG was written.
  private static func write(_ canvas: NSView, to path: String, appearance: NSAppearance?) -> Bool {
    guard let appearance, let data = PNG.data(of: canvas, appearance: appearance) else {
      return false
    }
    return (try? data.write(to: URL(fileURLWithPath: path))) != nil
  }
}
