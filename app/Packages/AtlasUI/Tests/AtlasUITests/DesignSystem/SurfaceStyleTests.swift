import SwiftUI
import Testing

@testable import AtlasUI

// On the main actor: a colour that follows the appearance resolves through AppKit, which answers on the main thread.
@Suite("Surface style")
@MainActor
struct SurfaceStyleTests {
  @Test("every surface is an opaque fill in the light and the dark, so Reduce Transparency has nothing to change")
  func everySurfaceIsOpaque() {
    for style in SurfaceStyle.allCases {
      for environment in [EnvironmentValues.light, .dark] {
        #expect(style.fill.resolve(in: environment).opacity == 1, "\(style) in \(environment.colorScheme)")
      }
    }
  }

  @Test("a floating surface and a modal one draw their own fills, the floating bar's and the alert's")
  func eachSurfaceDrawsItsFill() {
    #expect(SurfaceStyle.floating.fill == .fillFloating)
    #expect(SurfaceStyle.modal.fill == .fillModal)
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
