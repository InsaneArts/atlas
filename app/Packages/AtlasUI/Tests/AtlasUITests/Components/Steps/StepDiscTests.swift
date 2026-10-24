import SwiftUI
import Testing

@testable import AtlasUI

@Suite("StepDisc")
struct StepDiscTests {
  @Test(
    "a step's pop gives a finite scale at every moment, with or without a hold, so AppKit never gets a broken transform",
    arguments: [(0.0, 0.8), (0.0, 0.9), (0.12, 0.9), (0.0, 1.0)]
  )
  func popStaysFinite(hold: TimeInterval, from: CGFloat) throws {
    let timeline = KeyframeTimeline(initialValue: CGFloat(1)) {
      StepDisc.pop(hold: hold, from: from)
    }
    try #require(timeline.duration.isFinite, "the pop's length is \(timeline.duration)")
    let frames = Int((timeline.duration * 240).rounded(.up))
    for frame in 0...frames {
      let time = Double(frame) / 240
      let scale = timeline.value(time: time)
      #expect(scale.isFinite, "at \(time) s the disc's scale is \(scale)")
    }
    #expect(abs(timeline.value(time: timeline.duration) - 1) < 0.01, "it ends whole")
  }
}
