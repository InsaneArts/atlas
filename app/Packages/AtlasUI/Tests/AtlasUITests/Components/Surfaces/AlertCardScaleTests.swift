import Testing

@testable import AtlasUI

@Suite("AlertCardScale")
struct AlertCardScaleTests {
  @Test("an arriving card starts at 1.06 and settles at 1, and under Reduce Motion never scales")
  func arrivalScale() {
    #expect(AlertCardScale.arrival(settled: false, reduceMotion: false) == 1.06)
    #expect(AlertCardScale.arrival(settled: true, reduceMotion: false) == 1)
    #expect(AlertCardScale.arrival(settled: false, reduceMotion: true) == 1)
  }
}
