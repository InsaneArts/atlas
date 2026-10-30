import SwiftUI
import Testing

@testable import AtlasUI

@Suite("Activity disclosure visibility")
struct ActivityDisclosureTests {
  @Test("a step and its diagnostic remain invisible until the entire row fits above the disclosure edge")
  func cutRowsStayInvisible() {
    let row = CGRect(x: 0, y: 40, width: 200, height: 120)
    for edge in [0.0, 40, 100, 159, 160] {
      let bounds = CGRect(x: 0, y: 0, width: 200, height: edge)
      #expect(ActivityDisclosure.opacity(row: row, bounds: bounds) == 0)
    }
    #expect(ActivityDisclosure.opacity(row: row, bounds: CGRect(x: 0, y: 0, width: 200, height: 168)) == 1)
  }

  @Test("reversing a disclosure uses the same row opacity at the same height without pending animation steps")
  func disclosureReversalIsContinuous() {
    let row = CGRect(x: 0, y: 40, width: 200, height: 120)
    let edges = (0...8)
      .map { CGFloat(160 + $0) }
    let opening = edges.map { ActivityDisclosure.opacity(row: row, bounds: CGRect(x: 0, y: 0, width: 200, height: $0)) }
    let folding = edges.reversed()
      .map {
        ActivityDisclosure.opacity(row: row, bounds: CGRect(x: 0, y: 0, width: 200, height: $0))
      }
    #expect(opening == folding.reversed())
    #expect(opening == opening.sorted())
  }
}
