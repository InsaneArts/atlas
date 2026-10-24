import SwiftUI
import Testing

@testable import AtlasUI

@Suite("WorkingLabel")
struct WorkingLabelTests {
  @Test("a working label animates its own changes on the state curve, and only fades under Reduce Motion")
  func itAnimatesItsOwnChanges() {
    #expect(WorkingLabel.animation(reduceMotion: false) == Motion.state)
    #expect(WorkingLabel.animation(reduceMotion: true) == Motion.reduced)
    #expect(Motion.contentTransition(.interpolate, reduceMotion: false) == .interpolate)
    #expect(Motion.contentTransition(.interpolate, reduceMotion: true) == .opacity, "the words crossfade")
  }
}
