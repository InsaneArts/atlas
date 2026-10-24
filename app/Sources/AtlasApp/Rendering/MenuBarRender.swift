import AppKit
import AtlasStatus

/// The menu bar item's render, `--render-menubar DIR`, in release builds too: the item in every state, on a piece of
/// the light and the dark menu bar, to `DIR/<state>-<light|dark>.png`. The orbit is the status button's own image and
/// the badge the live ``StatusBadge`` layer, both drawn by ``MenuBarItemView``, held still as under Reduce Motion.
enum MenuBarRender {
  /// Each render's file name with the state it draws, every row of the status table. `scripts/render-app.mjs` keeps
  /// the same list; `tests/test_render_table.mjs` checks that it does.
  nonisolated static let states: [(file: String, kind: Mode.Kind)] = [
    ("idle", .idle),
    ("working", .working),
    ("control", .control),
    ("needsYou", .needsYou),
    ("stopped", .stopped),
    ("notReady", .notReady),
    ("loggedOut", .loggedOut),
    ("connecting", .connecting),
    ("inUseElsewhere", .inUseElsewhere),
  ]
  /// The two menu bars each state is drawn on, by file suffix: v3's menu bar colours for its light and dark cells.
  static let bars: [(suffix: String, appearance: NSAppearance?, ground: NSColor)] = [
    ("light", Appearance.light, NSColor(srgbRed: 236 / 255, green: 236 / 255, blue: 240 / 255, alpha: 1)),
    ("dark", Appearance.dark, NSColor(srgbRed: 28 / 255, green: 28 / 255, blue: 30 / 255, alpha: 1)),
  ]
  /// 24: the piece of menu bar drawn, a little taller than the 22-point item, which the bar centres.
  static let barHeight: CGFloat = 24
  /// 10: the bar on either side of the item.
  static let margin: CGFloat = 10

  /// Draws every state on both menu bars into `directory`, making the folder when it is missing.
  ///
  /// - Parameter directory: The folder the PNGs go to.
  /// - Returns: Whether every PNG was written.
  static func draw(to directory: String) -> Bool {
    let folder = URL(fileURLWithPath: directory)
    guard (try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)) != nil else {
      return false
    }
    return states.flatMap { state in
      bars.map { bar in
        draw(state.kind, on: bar.ground, appearance: bar.appearance)
          .flatMap { try? $0.write(to: folder.appendingPathComponent("\(state.file)-\(bar.suffix).png")) } != nil
      }
    }
    .allSatisfy { $0 }
  }

  /// One state on a piece of menu bar, as PNG data.
  ///
  /// - Parameters:
  ///   - kind: The state.
  ///   - ground: The menu bar's colour.
  ///   - appearance: The menu bar's appearance.
  /// - Returns: The PNG's bytes, or nil when none could be made.
  static func draw(_ kind: Mode.Kind, on ground: NSColor, appearance: NSAppearance?) -> Data? {
    guard let appearance else { return nil }
    let width = StatusItem.Layout.width + 2 * margin
    let canvas = NSView(frame: NSRect(x: 0, y: 0, width: width, height: barHeight))
    canvas.addSubview(PanelRender.fill(canvas.bounds, ground))
    let item = item(kind, appearance: appearance)
    item.setFrameOrigin(NSPoint(x: margin, y: (barHeight - item.frame.height) / 2))
    canvas.addSubview(item)
    return PNG.data(of: canvas, appearance: appearance)
  }

  /// The item as the menu bar shows it: its square, holding the button's image and the badge's layer over it.
  ///
  /// - Parameters:
  ///   - kind: The state.
  ///   - appearance: The menu bar's appearance, which settles the badge's colour.
  /// - Returns: The item's view, at the origin.
  static func item(_ kind: Mode.Kind, appearance: NSAppearance) -> NSView {
    let side = StatusItem.Layout.width
    let bounds = NSRect(x: 0, y: 0, width: side, height: side)
    let look = StatusGlyph.look(for: kind)
    // A template in the menu bar's own ink: white on the dark bar, black on the light one.
    let label = NSColor.labelColor
    var ink = label
    appearance.performAsCurrentDrawingAppearance {
      ink =
        label
        .usingColorSpace(.sRGB)?
        .withAlphaComponent(1) ?? label
    }
    let badge = StatusBadge()
    badge.show(look, in: bounds, flipped: false, appearance: appearance, motion: .still)
    return MenuBarItemView(
      image: StatusGlyph.image(for: look),
      opacity: StatusGlyph.opacity(of: look.tint),
      ink: ink,
      badge: badge.layer,
      frame: bounds
    )
  }
}
