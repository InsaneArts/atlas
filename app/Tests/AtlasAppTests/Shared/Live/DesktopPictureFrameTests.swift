import AppKit
import AtlasService
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("DesktopPicture+Frame")
struct DesktopPictureFrameTests {
  @Test("a PNG that names no resolution is a point a pixel, as the helper's are")
  func aPlainPNGIsAPointAPixel() async throws {
    let frame = Frame(png: png(width: 4, height: 3), id: "f", revision: "r", display: .init(width: 4, height: 3))
    let decoded = try #require(await DesktopPicture.decode(frame))
    #expect(decoded.size == CGSize(width: 4, height: 3))
    #expect(decoded.picture.image.width == 4)
  }

  @Test("a PNG that names its resolution is sized in points, as AppKit sizes it")
  func aRetinaPNGIsSizedInPoints() async throws {
    let data = png(width: 4, height: 2, points: CGSize(width: 2, height: 1))
    let frame = Frame(png: data, id: "f", revision: "r", display: .init(width: 2, height: 1))
    let decoded = try #require(await DesktopPicture.decode(frame))
    #expect(decoded.size == NSImage(data: data)?.size)
  }

  @Test("data that is not an image decodes to nothing")
  func notAnImageIsNothing() async {
    #expect(
      await DesktopPicture.decode(
        Frame(png: Data("x".utf8), id: "f", revision: "r", display: .init(width: 1, height: 1))
      )
        == nil
    )
  }
}
