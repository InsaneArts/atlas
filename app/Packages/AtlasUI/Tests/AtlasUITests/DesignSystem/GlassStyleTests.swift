import SwiftUI
import Testing

@testable import AtlasUI

// On the main actor: a colour that follows the appearance resolves through AppKit, which answers on the main thread.
@Suite("Glass style")
@MainActor
struct GlassStyleTests {
  @Test("glass is the system glass on macOS 26, the thin material before it, and opaque under Reduce Transparency")
  func backingFollowsTheSystemAndReduceTransparency() {
    #expect(GlassStyle.backing(glassAvailable: true, reduceTransparency: false) == .glass)
    #expect(GlassStyle.backing(glassAvailable: false, reduceTransparency: false) == .material)
    #expect(GlassStyle.backing(glassAvailable: true, reduceTransparency: true) == .opaque)
    #expect(GlassStyle.backing(glassAvailable: false, reduceTransparency: true) == .opaque)
  }

  @Test("an opaque glass draws an opaque fill, and the chrome's system glass draws none")
  func opaqueGlassIsOpaque() throws {
    let environment = EnvironmentValues()
    for style in GlassStyle.allCases {
      let fill = try #require(style.fill(on: .opaque), "\(style)")
      #expect(fill.resolve(in: environment).opacity == 1, "\(style)")
    }
    #expect(GlassStyle.regular.fill(on: .glass) == nil)
  }

  @Test("a modal surface is never the clear system glass, and draws its near-opaque tint on the material")
  func aModalSurfaceIsNearlyOpaque() throws {
    #expect(GlassStyle.thick.backing(glassAvailable: true, reduceTransparency: false) == .material)
    #expect(GlassStyle.regular.backing(glassAvailable: true, reduceTransparency: false) == .glass)
    #expect(GlassStyle.thick.backing(glassAvailable: true, reduceTransparency: true) == .opaque)
    let fill = try #require(GlassStyle.thick.fill(on: .material))
    #expect(fill.resolve(in: EnvironmentValues()).opacity >= 0.94)
  }

  @Test("the scrim behind an alert is black at one half in the dark and at one quarter in the light")
  func theScrimIsHalfBlackInTheDark() {
    for (environment, opacity) in [(EnvironmentValues.dark, Float(0.5)), (.light, 0.25)] {
      let scrim = Color.scrim
        .resolve(in: environment)
      #expect(abs(scrim.opacity - opacity) < 0.001)
      #expect(scrim.red == 0 && scrim.green == 0 && scrim.blue == 0)
    }
  }
}
