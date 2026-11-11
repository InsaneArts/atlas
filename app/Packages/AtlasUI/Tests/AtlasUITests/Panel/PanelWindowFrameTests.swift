import SwiftUI
import Testing

@testable import AtlasUI

@Suite("PanelWindowFrame")
struct PanelWindowFrameTests {
  @Test("the panel's window is its card with the gap under the menu bar above it and its shadow's room round it")
  func theWindowIsTheCardAndItsRoom() {
    #expect(PanelWindowFrame.top == 6)
    #expect(PanelWindowFrame.side == 68)
    #expect(PanelWindowFrame.size(card: 400) == CGSize(width: 488, height: 474))
    #expect(PanelWindowFrame.size(card: 399.5) == CGSize(width: 488, height: 474), "a part point rounds up")
  }

  @Test("the card sits under the gap, clear of the window's sides and lower edge by its shadow's room")
  func theCardSitsInsideItsRoom() {
    let window = PanelWindowFrame.size(card: 400)
    #expect(PanelWindowFrame.card(in: window) == CGRect(x: 68, y: 68, width: 352, height: 400))
  }
}
