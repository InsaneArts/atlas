import Testing

@testable import AtlasUI

@Suite("ViewerLayout")
struct ViewerLayoutTests {
  @Test("the sidebar waits below 1000 points, and shows only when wanted")
  func theSidebarWaitsForRoom() {
    #expect(ViewerLayout.showsSidebar(width: 1000, wanted: true))
    #expect(!ViewerLayout.showsSidebar(width: 999, wanted: true))
    #expect(!ViewerLayout.showsSidebar(width: 1400, wanted: false))
  }

  @Test("captions step aside only while the sidebar shows, not while it is wanted in a window too narrow for it")
  func captionsStepAsideOnlyBesideTheSidebar() {
    let caption = ActivityEntry(
      id: 0,
      initial: "A",
      line: "Agent ran a command",
      when: "just now",
      kind: .others,
      isNewest: true
    )
    let stage = ViewerStageState(isDimmed: false, isInControl: false, captions: [caption])
    let narrow = ViewerLayout.showsSidebar(width: 999, wanted: true)
    #expect(ViewerLayout.stage(stage, besideSidebar: narrow).captions == stage.captions)
    let wide = ViewerLayout.showsSidebar(width: 1000, wanted: true)
    #expect(ViewerLayout.stage(stage, besideSidebar: wide).captions.isEmpty)
  }
}
