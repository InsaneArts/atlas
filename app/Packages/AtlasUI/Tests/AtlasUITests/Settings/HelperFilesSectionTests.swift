import SwiftUI
import Testing

@testable import AtlasUI

@Suite("HelperFilesSection")
@MainActor
struct HelperFilesSectionTests {
  @Test("the section's layout moves on the state spring, and at once under Reduce Motion")
  func layoutMovesOnTheStateSpring() {
    #expect(HelperFilesSection.layoutAnimation(reduceMotion: false) == Motion.state)
    #expect(HelperFilesSection.layoutAnimation(reduceMotion: true) == nil)
  }

  @Test("the layout changes as rows or a failure's line come and go, never as words change in place")
  func layoutFollowsWhatShows() {
    let open = PreviewStates.settingsFilesOpen
    #expect(HelperFilesSection.layout(open) == HelperFilesSection.layout(PreviewStates.settingsFilesPrivate))
    #expect(HelperFilesSection.layout(open) != HelperFilesSection.layout(PreviewStates.settingsFilesFailed))
    var unknown = open
    unknown.home = nil
    #expect(HelperFilesSection.layout(open) != HelperFilesSection.layout(unknown))
  }
}
