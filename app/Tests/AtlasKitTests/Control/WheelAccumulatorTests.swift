import Testing

@testable import AtlasKit

@Suite("WheelAccumulator")
struct WheelAccumulatorTests {
  @Test("one send carries at most a hundred lines each way")
  func capped() {
    var wheel = WheelAccumulator()
    wheel.add(dx: -80000, dy: 80000, precise: true)
    #expect(wheel.take() == (-100, 100))
  }

  @Test("small trackpad movement accumulates across sends and reversals")
  func accumulates() {
    var wheel = WheelAccumulator()
    var total = 0
    for _ in 0..<30 {
      wheel.add(dx: 0, dy: -2, precise: true)
      total += wheel.take().dy
    }
    #expect(total == -6)
    for _ in 0..<30 {
      wheel.add(dx: 0, dy: 2, precise: true)
      total += wheel.take().dy
    }
    #expect(total == 0)
  }

  @Test("a wheel notch is sent as one line")
  func notch() {
    var wheel = WheelAccumulator()
    wheel.add(dx: 0, dy: 3, precise: false)
    #expect(wheel.take() == (0, 3))
  }
}
