import Testing

@testable import AtlasUI

@Suite("Shake")
struct ShakeTests {
  @Test("a refusal swings left 5, right 5, left 3, right 3 and home, and under Reduce Motion stays home")
  func aRefusalSwingsUnlessReduceMotion() {
    #expect(Shake.swings(reduceMotion: false) == [-5, 5, -3, 3, 0])
    // Reduce Motion changes the swings, never the trigger, so turning it on or off sets no shake off.
    #expect(Shake.swings(reduceMotion: true) == [0, 0, 0, 0, 0])
  }
}
