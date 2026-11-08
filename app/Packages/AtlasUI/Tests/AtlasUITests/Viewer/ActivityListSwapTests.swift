import SwiftUI
import Testing

@testable import AtlasUI

@Suite("Activity list swap")
struct ActivityListSwapTests {
  @Test("another list fades in once the old one has faded at the top, and only fades in when scrolled down")
  func anotherListSwapsInWhole() {
    #expect(ActivityMotion.swapOut(isAtTop: true, reduceMotion: false) == Motion.swapOut)
    #expect(ActivityMotion.swapIn(isAtTop: true, reduceMotion: false) == Motion.swapInAfterOut)
    #expect(Motion.swapInAfterOut == Animation.easeOut(duration: 0.15).delay(0.1), "in only once the old is out")
    #expect(ActivityMotion.swapOut(isAtTop: false, reduceMotion: false) == nil, "the old list never jumps to the top")
    #expect(ActivityMotion.swapIn(isAtTop: false, reduceMotion: false) == Motion.swapIn)
  }

  @Test("under Reduce Motion another list takes the old one's place with a short fade and nothing else")
  func anotherListOnlyFadesUnderReduceMotion() {
    for isAtTop in [true, false] {
      #expect(ActivityMotion.swapOut(isAtTop: isAtTop, reduceMotion: true) == nil)
      #expect(ActivityMotion.swapIn(isAtTop: isAtTop, reduceMotion: true) == Motion.crossfadeReduced)
    }
    #expect(Motion.crossfadeReduced == Animation.easeOut(duration: 0.15))
  }

  @MainActor
  @Test("the list is another one for another session or tab, and the same one as the search narrows it")
  func theListFollowsTheSessionAndTheTab() {
    var state = PreviewStates.oneSessionActivity
    let list = state.list
    #expect(list == ViewerActivityListKey(session: .one(SessionSamples.claudeKey), filter: .all))
    state.query = "Chrome"
    state.days.removeAll()
    #expect(state.list == list, "a search or new work keeps the list")
    state.filters.selected = .typing
    #expect(state.list == ViewerActivityListKey(session: .one(SessionSamples.claudeKey), filter: .typing))
    state.session = .all
    #expect(state.list == ViewerActivityListKey(session: .all, filter: .typing))
  }
}
