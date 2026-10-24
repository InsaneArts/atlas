import SwiftUI
import Testing

@testable import AtlasUI

@Suite("StatusLine")
struct StatusLineTests {
  @Test("a status line paints a problem in the attention colour and done in the success colour")
  func problemAndDoneColours() {
    #expect(StatusLine.color(.problem) == .textAttention)
    #expect(StatusLine.color(.done) == .textSuccess)
    #expect(StatusLine.color(.working) == .textSecondary)
    #expect(OutcomeMark.color(.problem) == .attention)
    #expect(OutcomeMark.color(.done) == .success)
  }

  @Test("a status line tells a problem from done in words, not by colour alone")
  func spokenKinds() {
    #expect(StatusLine.Kind.problem.spokenValue == "Problem")
    #expect(StatusLine.Kind.done.spokenValue == "Done")
    #expect(StatusLine.Kind.working.spokenValue != StatusLine.Kind.problem.spokenValue)
  }
}
