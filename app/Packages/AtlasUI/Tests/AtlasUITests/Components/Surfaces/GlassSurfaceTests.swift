import SwiftUI
import Testing

@testable import AtlasUI

@Suite("GlassSurface")
struct GlassSurfaceTests {
  @Test("glass falls back to an opaque fill under Reduce Transparency")
  func glassTurnsOpaqueUnderReduceTransparency() throws {
    let environment = EnvironmentValues()
    for drawsGlassEffect in [true, false] {
      let backing = GlassStyle.backing(
        glassAvailable: true,
        reduceTransparency: true,
        drawsGlassEffect: drawsGlassEffect
      )
      for style in GlassStyle.allCases {
        let fill = try #require(style.fill(on: backing), "\(style)")
        #expect(fill.resolve(in: environment).opacity == 1, "\(style)")
      }
    }
  }

  @Test("a render that cannot draw the system glass gets the material, never a blank")
  func renderDrawsTheMaterial() {
    #expect(GlassStyle.backing(glassAvailable: true, reduceTransparency: false, drawsGlassEffect: false) == .material)
    #expect(GlassStyle.backing(glassAvailable: true, reduceTransparency: false) == .glass)
  }
}
