import CoreGraphics
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
}
