import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerFramePump")
struct ViewerFramePumpTests {
  @Test("the very same pixels again reuse the last image under the new frame's key")
  func theSamePixelsAgainReuseTheLastImage() async throws {
    let rgb = Data(repeating: 7, count: 2 * 2 * 3)
    let first = try #require(await ViewerFramePump.decode(rgb: rgb, width: 2, height: 2, key: "one", reusing: nil))
    let kept = DecodedPixels(rgb: rgb, decoded: first)
    let again = try #require(
      await ViewerFramePump.decode(rgb: Data(rgb), width: 2, height: 2, key: "two", reusing: kept)
    )
    #expect(again.picture.image === first.picture.image)
    #expect(again.picture.id == "two")
    #expect(again.size == first.size)
  }

  @Test("other pixels, or another size, decode a new image")
  func otherPixelsDecodeANewImage() async throws {
    let rgb = Data(repeating: 7, count: 2 * 2 * 3)
    let first = try #require(await ViewerFramePump.decode(rgb: rgb, width: 2, height: 2, key: "one", reusing: nil))
    let kept = DecodedPixels(rgb: rgb, decoded: first)
    var changed = rgb
    changed[5] = 8
    let other = try #require(
      await ViewerFramePump.decode(rgb: changed, width: 2, height: 2, key: "two", reusing: kept)
    )
    #expect(other.picture.image !== first.picture.image)
    let reshaped = try #require(
      await ViewerFramePump.decode(rgb: rgb, width: 4, height: 1, key: "three", reusing: kept)
    )
    #expect(reshaped.picture.image !== first.picture.image)
    #expect(reshaped.size == CGSize(width: 4, height: 1))
  }
}
