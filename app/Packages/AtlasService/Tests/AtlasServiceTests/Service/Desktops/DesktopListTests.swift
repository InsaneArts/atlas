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

  @Test("a VM's facts read its power, screen, disk, macOS, maintenance and Atlas, and an odd fact stays unsaid")
  func aVMsFacts() throws {
    let reply = Data(
      #"""
      {"ok": true, "desktops": [
        {"id": "b2", "kind": "vm", "facts": {"can": ["stop"], "power": "running",
          "display": {"width": 1920, "height": 1200}, "disk": {"used": 28000000000, "size": 128000000000},
          "macos": {"version": "27.2.1", "build": "27C5", "latest": {"version": "27.3", "build": "27D1"}, "newer": true},
          "maintenance": {"state": "on", "reason": "work is under maintenance"},
          "atlas": {"state": "available", "next": "Run atlas update."}}},
        {"id": "c3", "kind": "vm", "facts": {"power": 7, "display": {"width": -1, "height": 9}, "disk": {"used": null,
          "size": 128000000000}, "macos": {"version": null, "latest": null, "newer": null}}},
        {"id": "a1", "kind": "user"}
      ]}
      """#.utf8
    )
    let decoder = JSONDecoder()
    let list = try decoder.decode(DesktopList.self, from: reply)
    let work = try #require(list.desktops.first?.facts)
    #expect(
      work
        == DesktopList.Facts(
          power: "running",
          display: ViewerStatus.Size(width: 1920, height: 1200),
          diskUsed: 28_000_000_000,
          diskSize: 128_000_000_000,
          macOS: "27.2.1",
          latestMacOS: "27.3",
          newerMacOS: true,
          maintenance: "on",
          atlas: "available",
          atlasNext: "Run atlas update."
        )
    )
    #expect(list.desktops.first?.can == ["stop"])
    let odd = try #require(list.desktops[1].facts)
    #expect(odd == DesktopList.Facts(diskSize: 128_000_000_000), "nothing odd reads as said")
    #expect(list.desktops.last?.facts == nil, "Second Login has no facts")
  }
}
