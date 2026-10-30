import Foundation
import Testing

@testable import AtlasService

@Suite("Complete helper occluders")
struct HelperOccluderListTests {
  @Test("the complete command list retains menus and accessory geometry in its original order")
  func allCoveringGeometryIsRetained() throws {
    let data = Data(
      #"{"windows":[{"number":9,"owner":"Menu","alpha":1,"bounds":{"x":0,"y":0,"width":200,"height":100}},{"number":7,"owner":"Accessory","alpha":1,"bounds":{"x":0,"y":0,"width":400,"height":200}},{"number":1,"alpha":1,"bounds":{"x":0,"y":0,"width":400,"height":300}}]}"#
        .utf8
    )
    let list = try JSONDecoder()
      .decode(HelperOccluderList.self, from: data)
    let windows = list.covering(HelperDisplay(width: 400, height: 300))
    #expect(windows.map(\.id) == [9, 7, 1])
    #expect(windows.map(\.order) == [0, 1, 2])
    let target = try #require(windows.last)
    #expect(!HelperWindowVisibility.isInView(target, among: windows, display: HelperDisplay(width: 400, height: 300)))
  }

  @Test("a malformed occluder fails the complete observation rather than disappearing from it")
  func incompleteObservationsAreRefused() {
    let data = Data(
      #"{"windows":[{"number":1,"alpha":1,"bounds":{"x":0,"y":0,"width":100,"height":100}},{"number":2}]}"#.utf8
    )
    #expect(throws: DecodingError.self) {
      try JSONDecoder()
        .decode(HelperOccluderList.self, from: data)
    }
  }
}
