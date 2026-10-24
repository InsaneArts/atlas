import Testing

@testable import AtlasUI

@Suite("SettingsLayout")
struct SettingsLayoutTests {
  @Test("Check Again staggers each row's mark 60 milliseconds after the last")
  func checkAgainStaggersEachMark() {
    #expect(SettingsLayout.checkStagger(index: 0) == 0, "the first mark pops at once")
    for index in 1..<4 {
      let gap = SettingsLayout.checkStagger(index: index) - SettingsLayout.checkStagger(index: index - 1)
      #expect(abs(gap - 0.06) < 1e-9, "row \(index)")
    }
  }
}
