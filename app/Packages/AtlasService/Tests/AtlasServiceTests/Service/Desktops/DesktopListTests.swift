import Foundation
import Testing

@testable import AtlasService

@Suite("Desktop list")
struct DesktopListTests {
  @Test("an odd desktop is left out and every missing field keeps its default")
  func oddDesktopsAreLeftOut() throws {
    let reply = Data(
      #"""
      {"ok": true, "viewed": 7, "desktops": [
        {"name": "no id"},
        7,
        {"id": "c3", "kind": "cloud", "standing": "working", "facts": "none"}
      ]}
      """#.utf8
    )
    let decoder = JSONDecoder()
    let list = try decoder.decode(DesktopList.self, from: reply)
    #expect(list.viewed == nil)
    #expect(list.desktops.count == 1)
    let odd = try #require(list.desktops.first)
    #expect(odd.id == "c3")
    #expect(odd.kind == .unknown("cloud"))
    #expect(odd.standing == .working)
    #expect(odd.can.isEmpty)
    #expect(!odd.isDefault && !odd.isViewed)
  }
}
