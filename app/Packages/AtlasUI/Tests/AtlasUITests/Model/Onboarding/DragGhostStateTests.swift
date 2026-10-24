import CoreGraphics
import Foundation
import Testing

@testable import AtlasUI

@Suite("DragGhostState")
struct DragGhostStateTests {
  @Test("the ghost lies on the straight path from the icon to the list")
  func pointsAlongThePath() {
    let state = DragGhostState(from: CGPoint(x: 100, y: 300), to: CGPoint(x: 500, y: 100), start: .now)
    #expect(state.point(at: 0) == state.from)
    #expect(state.point(at: 1) == state.to)
    #expect(state.point(at: 0.5) == CGPoint(x: 300, y: 200))
  }
}
