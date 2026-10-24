import CoreGraphics
import Foundation
import Testing

@testable import AtlasApp

@MainActor
@Suite("DragGhost")
struct DragGhostTests {
  @Test("the overlay covers both windows past their edges, and the path runs from the icon into the list")
  func path() {
    let settings = CGRect(x: 200, y: 300, width: 700, height: 600)
    let window = CGRect(x: 916, y: 400, width: 360, height: 500)
    let icon = CGRect(x: 1060, y: 760, width: 72, height: 72)
    let start = Date(timeIntervalSince1970: 1_000)
    let (area, state) = DragGhost.path(icon: icon, settings: settings, window: window, start: start)
    let margin = DragGhost.margin
    #expect(area == CGRect(x: 200 - margin, y: 300 - margin, width: 1076 + margin * 2, height: 600 + margin * 2))
    #expect(state.from == CGPoint(x: icon.midX - area.minX, y: area.maxY - icon.midY))
    let landing = CGPoint(
      x: settings.minX + settings.width * DragGhost.landing.x,
      y: settings.maxY - settings.height * DragGhost.landing.y
    )
    #expect(state.to == CGPoint(x: landing.x - area.minX, y: area.maxY - landing.y))
    #expect(state.to.x < state.from.x, "into System Settings, at the panel's left")
    #expect(state.start == start)
  }
}
