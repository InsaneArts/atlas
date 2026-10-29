import SwiftUI
import Testing

@testable import AtlasUI

@Suite("ActivityScroll")
struct ActivityScrollTests {
  @Test("a new line fades in while the list sits at its top, and nothing moves once it is scrolled down")
  func newLinesMoveOnlyAtTheTop() {
    #expect(ActivityScroll.moving(isAtTop: true, reduceMotion: false) == Motion.state)
    #expect(Motion.Springs.state.bounce == 0)
    #expect(ActivityScroll.moving(isAtTop: false, reduceMotion: false) == nil)
    #expect(ActivityScroll.moving(isAtTop: true, reduceMotion: true) == nil)
  }
}
