import CoreGraphics
import SwiftUI
import Testing

@testable import AtlasUI

@Suite("ListRow")
struct ListRowTests {
  @Test("a list row's separator starts where its title starts")
  func separatorStartsAtTheTitle() {
    #expect(ListRow.separatorInset(.none) == 14)
    #expect(ListRow.separatorInset(.check(.done)) == 42)
    for lead in [ListRow.Lead.tile(.text(">_")), .picture, .letter("T")] {
      #expect(ListRow.separatorInset(lead) == 56, "\(lead)")
    }
  }

  @Test("the setup checklist keeps a setup list's inset, where a Settings row sits two points in")
  func checklistKeepsTheSetupInset() {
    #expect(FinishingScreen.rowSize.height == 44)
    #expect(FinishingScreen.rowSize.inset == 14)
    #expect(ListRow.separatorInset(.check(.done), size: FinishingScreen.rowSize) == 42)
    #expect(ListRow.separatorInset(.check(.done), size: .compact) == 40)
    #expect(ListRow.titleWeight(.check(.done), size: FinishingScreen.rowSize) == .regular)
  }
}
