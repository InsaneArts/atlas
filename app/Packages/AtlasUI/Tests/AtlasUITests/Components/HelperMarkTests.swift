import Testing

@testable import AtlasUI

@Suite("HelperMark")
struct HelperMarkTests {
  @Test("the helper's icon stands the size of the apps listed above it, and its dot is 8 in a 2-point ring")
  func theMarkKeepsItsProportions() {
    #expect(HelperMarkLayout.rowTile == SessionIcon.plate(SessionIcon.rowSide))
    #expect(HelperMarkLayout.dot == 8)
    #expect(HelperMarkLayout.ring == 2)
    #expect(HelperMarkLayout.reach == 12)
    #expect(HelperMarkLayout.cardTile > HelperMarkLayout.rowTile, "the Helper page's card shows it larger")
  }
}
