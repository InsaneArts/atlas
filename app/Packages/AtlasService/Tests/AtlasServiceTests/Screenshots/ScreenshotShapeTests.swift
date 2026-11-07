import Testing

@testable import AtlasService

@Suite("Screenshot shape")
struct ScreenshotShapeTests {
  @Test("the largest 16:10 screenshot fills its frame and a window or an unsized one stands inside it")
  func theLargestSixteenByTenFills() {
    let listed = [
      Screenshot(name: "screen.png", width: 2880, height: 1800),
      Screenshot(name: "window.png", width: 672, height: 438),
      Screenshot(name: "small.png", width: 1440, height: 900),
      Screenshot(name: "unsized.png"),
    ]
    #expect(ScreenshotShape.fillsFrame("screen.png", in: listed))
    #expect(!ScreenshotShape.fillsFrame("window.png", in: listed))
    #expect(!ScreenshotShape.fillsFrame("small.png", in: listed), "a smaller 16:10 picture is a window")
    #expect(!ScreenshotShape.fillsFrame("unsized.png", in: listed))
    #expect(!ScreenshotShape.fillsFrame("missing.png", in: listed))
    let wide = [Screenshot(name: "wide.png", width: 2560, height: 1440)]
    #expect(!ScreenshotShape.fillsFrame("wide.png", in: wide), "filling would cut a 16:9 screen")
  }
}
