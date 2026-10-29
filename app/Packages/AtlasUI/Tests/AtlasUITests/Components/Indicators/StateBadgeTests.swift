import SwiftUI
import Testing

@testable import AtlasUI

@Suite("StateBadge")
struct StateBadgeTests {
  @Test(
    "a badge's colour is the accent while acting, the ground with a ring while waiting, and the attention colour while it needs you"
  )
  func aBadgeTakesItsStatesColour() {
    let ground = Color.groundRecessed
    #expect(StateBadge.fill(for: .acting, ground: ground) == .accent)
    #expect(StateBadge.innerRing(for: .acting) == .clear)
    #expect(StateBadge.fill(for: .waiting, ground: ground) == ground)
    #expect(StateBadge.innerRing(for: .waiting) == .textTertiary)
    #expect(StateBadge.fill(for: .needsYou, ground: ground) == .attention)
    #expect(StateBadge.innerRing(for: .needsYou) == .clear)
  }

  @Test("a badge arrives on the arrive spring and leaves eased in, and only fades under Reduce Motion")
  func aBadgeComesAndGoes() {
    #expect(StateBadge.appearance(isShown: true, reduceMotion: false) == Motion.arrive)
    #expect(StateBadge.appearance(isShown: false, reduceMotion: false) == Motion.leave)
    for isShown in [true, false] {
      #expect(StateBadge.appearance(isShown: isShown, reduceMotion: true) == Motion.reduced)
    }
  }
}
