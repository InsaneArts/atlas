import CoreGraphics
import Testing

@testable import AtlasUI

@Suite("ActivityStepPicture")
struct ActivityStepPictureTests {
  /// A step picture's crop.
  static let crop = CGSize(width: 80, height: 50)

  @Test("a spot in the middle stands in the crop's middle, the picture zoomed in 2.6 times")
  func spotCentred() {
    let frame = ActivityStepPicture.frame(aspect: 1.6, focus: CGPoint(x: 0.5, y: 0.5), in: Self.crop)
    #expect(abs(frame.width - Self.crop.width * ViewerLayout.stepPictureZoom) < 0.001)
    #expect(abs(frame.midX - Self.crop.width / 2) < 0.001)
    #expect(abs(frame.midY - Self.crop.height / 2) < 0.001)
  }

  @Test("a spot near an edge keeps the crop on the picture, never past its edge")
  func edgeKept() {
    let frame = ActivityStepPicture.frame(aspect: 1.6, focus: CGPoint(x: 0.98, y: 0.02), in: Self.crop)
    #expect(frame.maxX >= Self.crop.width - 0.001)
    #expect(frame.minY <= 0.001 && frame.minY >= -0.001)
  }

  @Test("a wide element zooms in only as far as it stays whole in the crop, around its middle")
  func wideElementKeptWhole() {
    let area = CGRect(x: 0.24, y: 0.79, width: 0.5, height: 0.15)
    let frame = ActivityStepPicture.frame(aspect: 1.6, focus: CGPoint(x: 0.49, y: 0.865), area: area, in: Self.crop)
    #expect(abs(frame.width * area.width - Self.crop.width) < 0.001, "the element fills the crop's width")
    #expect(abs(frame.minX + frame.width * 0.49 - Self.crop.width / 2) < 0.001)
  }

  @Test("a small element zooms in no further than a spot does")
  func smallElementZoomCapped() {
    let area = CGRect(x: 0.45, y: 0.45, width: 0.05, height: 0.04)
    let frame = ActivityStepPicture.frame(aspect: 1.6, focus: CGPoint(x: 0.475, y: 0.47), area: area, in: Self.crop)
    #expect(abs(frame.width - Self.crop.width * ViewerLayout.stepPictureZoom) < 0.001)
  }

  @Test("with no spot the whole picture fills the crop")
  func wholePicture() {
    let frame = ActivityStepPicture.frame(aspect: 1.6, focus: nil, in: Self.crop)
    #expect(frame == CGRect(origin: .zero, size: Self.crop))
  }
}
