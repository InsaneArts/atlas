import CoreGraphics
import Testing

@testable import AtlasService

@Suite("Screenshot shape")
struct ScreenshotShapeTests {
  @Test("a capture shaped like a near-16:10 display fills its frame, and a window or an unsized one stands inside it")
  func theWholeScreenFills() {
    let display = CGSize(width: 1512, height: 982)
    #expect(ScreenshotShape.fillsFrame(Screenshot(name: "s", width: 3024, height: 1964), display: display))
    #expect(ScreenshotShape.fillsFrame(Screenshot(name: "s", width: 1512, height: 982), display: display))
    #expect(!ScreenshotShape.fillsFrame(Screenshot(name: "w", width: 672, height: 438), display: display), "a window")
    #expect(!ScreenshotShape.fillsFrame(Screenshot(name: "u"), display: display))
    #expect(!ScreenshotShape.fillsFrame(nil, display: display))
    #expect(!ScreenshotShape.fillsFrame(Screenshot(name: "s", width: 1512, height: 982), display: nil))
  }

  @Test("the whole of a 16:9 screen stands inside its frame, since filling would cut it")
  func aWideScreenStandsInside() {
    let wide = CGSize(width: 2560, height: 1440)
    #expect(!ScreenshotShape.fillsFrame(Screenshot(name: "s", width: 2560, height: 1440), display: wide))
  }
}
