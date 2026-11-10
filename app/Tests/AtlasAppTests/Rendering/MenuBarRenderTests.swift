import AppKit
import AtlasStatus
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("MenuBarRender", .serialized, AppKitState())
struct MenuBarRenderTests {
  @Test("the menu bar renders draw every state once for each icon, under names of their own")
  func everyStateHasOneRender() {
    let kinds = MenuBarRender.states
      .map(\.kind)
    #expect(kinds.count == Mode.Kind.allCases.count)
    #expect(Set(MenuBarRender.states.map(\.file)).count == MenuBarRender.states.count)
    for kind in Mode.Kind.allCases {
      #expect(kinds.contains(kind), "\(kind) has no render")
    }
    #expect(MenuBarRender.bars.map(\.suffix) == ["light", "dark"])
    #expect(MenuBarRender.icons.map(\.icon) == MenuBarIcon.allCases)
    #expect(MenuBarRender.icons.map(\.prefix) == ["", "screen-"], "the Orbit's renders keep their names")
  }

  @Test(
    "every menu bar render draws the item on a piece of the bar, light and dark",
    arguments: MenuBarRender.states.map(\.file)
  )
  func everyRenderDraws(file: String) throws {
    let kind = try #require(MenuBarRender.states.first { $0.file == file }?.kind)
    for icon in MenuBarIcon.allCases {
      for bar in MenuBarRender.bars {
        let data = try #require(MenuBarRender.draw(kind, icon: icon, on: bar.ground, appearance: bar.appearance))
        let image = try #require(NSBitmapImageRep(data: data), "\(icon) \(file)-\(bar.suffix)")
        #expect(image.pixelsWide == Int(StatusItem.Layout.width + 2 * MenuBarRender.margin) * PNG.scale)
        #expect(image.pixelsHigh == Int(MenuBarRender.barHeight) * PNG.scale)
      }
    }
  }

  /// A render's colour at a point of the bar, top down.
  ///
  /// - Parameters:
  ///   - kind: The state.
  ///   - icon: The glyph.
  ///   - bar: The bar's file suffix.
  ///   - point: The point in the bar, top down.
  /// - Returns: The colour there, in sRGB.
  /// - Throws: When the render could not be drawn.
  private static func colour(
    _ kind: Mode.Kind,
    icon: MenuBarIcon = .orbit,
    on bar: String,
    at point: CGPoint
  ) throws -> NSColor {
    let found = try #require(MenuBarRender.bars.first { $0.suffix == bar })
    let data = try #require(MenuBarRender.draw(kind, icon: icon, on: found.ground, appearance: found.appearance))
    let image = try #require(NSBitmapImageRep(data: data))
    let pixel = try #require(
      image.colorAt(x: Int(point.x * CGFloat(PNG.scale)), y: Int(point.y * CGFloat(PNG.scale)))
    )
    return try #require(pixel.usingColorSpace(.sRGB))
  }

  @Test("the renders draw the badge in its colour at the orbit's lower right and the orbit in the bar's ink")
  func theRendersDrawTheItemsColours() throws {
    // The item sits 10 in and 1 down; the badge's centre is 20.5, 16.5 in it, the sphere's 12, 11, and Screen's
    // sphere 12, 9.5.
    let badge = CGPoint(x: 30.5, y: 17.5)
    let sphere = CGPoint(x: 22, y: 12)
    for icon in MenuBarIcon.allCases {
      for bar in ["light", "dark"] {
        let blue = try Self.colour(.working, icon: icon, on: bar, at: badge)
        #expect(blue.blueComponent > 0.9 && blue.redComponent < 0.2, "\(icon) working-\(bar)")
        let orange = try Self.colour(.stopped, icon: icon, on: bar, at: badge)
        #expect(orange.redComponent > 0.9 && orange.blueComponent < 0.3, "\(icon) stopped-\(bar)")
      }
    }
    let screen = try Self.colour(.idle, icon: .screen, on: "dark", at: CGPoint(x: 22, y: 10.5))
    #expect(screen.redComponent > 0.95 && screen.greenComponent > 0.95, "Screen's sphere in the bar's ink")
    let white = try Self.colour(.idle, on: "dark", at: sphere)
    #expect(white.redComponent > 0.95 && white.greenComponent > 0.95 && white.blueComponent > 0.95)
    let black = try Self.colour(.idle, on: "light", at: sphere)
    #expect(black.redComponent < 0.05 && black.greenComponent < 0.05 && black.blueComponent < 0.05)
    let control = try Self.colour(.control, on: "dark", at: sphere)
    #expect(control.redComponent > 0.95 && control.blueComponent > 0.95, "control's orbit keeps the bar's ink")
    let controlBadge = try Self.colour(.control, on: "dark", at: badge)
    #expect(controlBadge.blueComponent > 0.9 && controlBadge.redComponent < 0.2, "control's badge is a blue dot")
  }
}
