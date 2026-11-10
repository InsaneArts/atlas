import SwiftUI
import Testing

@testable import AtlasUI

@Suite("PrivacyPage")
@MainActor
struct PrivacyPageTests {
  @Test("the page's layout moves on the state spring, and at once under Reduce Motion")
  func layoutMovesOnTheStateSpring() {
    #expect(PrivacyPage.layoutAnimation(reduceMotion: false) == Motion.state)
    #expect(PrivacyPage.layoutAnimation(reduceMotion: true) == nil)
  }

  @Test("the layout changes as rows or a failure's line come and go, never as words change in place")
  func layoutFollowsWhatShows() {
    let open = PreviewStates.settingsFilesOpen
    #expect(PrivacyPage.layout(open) == PrivacyPage.layout(PreviewStates.settingsFilesPrivate))
    #expect(PrivacyPage.layout(open) != PrivacyPage.layout(PreviewStates.settingsFilesFailed))
    var unknown = open
    unknown.home = nil
    #expect(PrivacyPage.layout(open) != PrivacyPage.layout(unknown))
  }
}
