import Testing

@testable import AtlasControl

@Suite("WheelAccumulator")
struct WheelAccumulatorTests {
  @Test("one send carries at most a hundred lines each way")
  func capped() {
    var wheel = WheelAccumulator()
    wheel.add(dx: -80000, dy: 80000, precise: true)
    #expect(wheel.take() == (-100, 100))
  }

  @Test("lines past one send's most wait as whole lines for the next, and none wait once it is sent")
  func leftoverLinesWait() {
    var wheel = WheelAccumulator()
    wheel.add(dx: 0, dy: 150.5, precise: false)
    #expect(wheel.take() == (0, 100))
    #expect(wheel.holdsWholeLines)
    #expect(wheel.take() == (0, 50))
    #expect(!wheel.holdsWholeLines, "half a line is not a line to send")
  }

  @Test("a new gesture drops the fraction the last one left and keeps its whole lines not yet sent")
  func newGestureDropsTheFraction() {
    var wheel = WheelAccumulator()
    wheel.add(dx: 0, dy: 15, precise: true)
    #expect(wheel.take() == (0, 1))
    wheel.beginGesture()
    wheel.add(dx: 0, dy: 6, precise: true)
    #expect(wheel.take() == (0, 0), "the first gesture's half line does not complete the second's")
    wheel.add(dx: 0, dy: 2.5, precise: false)
    wheel.beginGesture()
    #expect(wheel.take() == (0, 3), "0.6 and 2.5 lines hold three whole ones")
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
