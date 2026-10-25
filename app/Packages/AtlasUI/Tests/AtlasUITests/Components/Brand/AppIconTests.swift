import CoreGraphics
import Testing

@testable import AtlasUI

@Suite("AppIcon")
struct AppIconTests {
  @Test("the app icon's canvas is its tile times 1024 over 824, and it is always square")
  func canvasIsTheTileWithItsMargin() {
    for tile in [CGFloat(88), 84, 80, 52, 48] {
      let canvas = AppIcon.canvas(tile: tile)
      #expect(abs(canvas.width - tile * 1024 / 824) < 0.001, "\(tile)")
      #expect(canvas.width == canvas.height, "\(tile)")
    }
    #expect(abs(AppIcon.canvas(tile: 52).width - 64.62) < 0.01)
  }
}
