import SwiftUI
import Testing

@testable import AtlasUI

@Suite("Glass style")
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
}
