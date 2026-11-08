import Testing

@testable import AtlasUI

@Suite("Nested Viewer Activity")
struct ViewerActivityStateTests {
  @MainActor
  @Test("a run's steps can only exist inside their run, and the newest run is the first session's first")
  func stepsBelongToTheirRun() {
    var state = PreviewStates.openedActivity
    let session = state.days[0].sessions[0]
    #expect(session.runs.map(\.name) == ["TextEdit", "Commands", "Google Chrome", "TextEdit"])
    #expect(state.newest == "run.textedit.now")
    let chrome = session.runs[2]
    #expect(chrome.isOpen && chrome.steps.map(\.id).first == "step.chrome.open", "an opened run reads oldest first")
    #expect(session.runs.filter { !$0.isOpen }.allSatisfy { $0.steps.isEmpty })
    state.days[0].sessions[0].runs.removeFirst()
    #expect(state.newest == "run.commands")
    state.days.removeAll()
    #expect(state.newest == nil)
  }

  @MainActor
  @Test("a long day keeps its headings, its sessions in order and the row for the rest")
  func aLongDayKeepsItsOrder() {
    let days = PreviewStates.daysActivity.days
    #expect(days.map(\.title) == ["Yesterday", "Wednesday"])
    #expect(days[1].sessions.map(\.project) == ["atlas-t2-r2", "atlas-t3-r3", "atlas-t4-r1"])
    #expect(days[1].more == ViewerMoreSessions(day: "wednesday", title: "46 more sessions on Wednesday"))
    let ended = days.flatMap(\.sessions)
    #expect(ended.allSatisfy { !$0.isOpen && $0.strip != nil }, "ended sessions fold to pictures")
  }

  @Test("a strip links to every picture only while it holds more than it shows")
  func aStripLinksOnlyWhenItHoldsMore() {
    let tile = ViewerPictureTile(id: "a", label: "Screenshot")
    #expect(ViewerPictureStrip(run: "r", tiles: [tile], total: 120).link == "120 pictures")
    #expect(ViewerPictureStrip(run: "r", tiles: [tile], total: 120).hasLink)
    #expect(!ViewerPictureStrip(run: "r", tiles: [tile], total: 1).hasLink)
    #expect(ViewerPictureGrid(run: "r", tiles: [tile], more: 112).moreTitle == "Show 112 more")
  }

  @Test("the sheet shows the picture before while there is none after, whichever side was picked")
  func theSheetShowsBeforeWithoutAnAfter() {
    var sheet = ViewerPictureSheet(
      id: "s",
      icon: .atlas,
      title: ViewerStepWords("Took a screenshot"),
      line: "",
      picture: nil,
      aspect: .zero,
      side: .after,
      position: "1 of 1",
      run: "",
      canGoBack: false,
      canGoForward: false,
      canShowInFinder: false
    )
    #expect(sheet.shownSide == .before)
    sheet.hasAfter = true
    #expect(sheet.shownSide == .after)
  }

  @MainActor
  @Test("a folded run with problem lines or pictures is a box with them, and an open or bare one a plain row")
  func foldedRunsWithKeyItemsAreBoxes() {
    #expect(PreviewStates.typingRun.isBoxed, "pictures under its row")
    #expect(PreviewStates.chromeRun.isBoxed, "a problem line and a picture")
    #expect(!PreviewStates.chromeRunOpen.isBoxed, "an open run is a plain row")
    var bare = PreviewStates.typingRun
    bare.strip = nil
    #expect(!bare.isBoxed, "nothing under its row")
    #expect(PreviewStates.terminalSession.isBoxed, "an ended session folded to its picture")
    #expect(!PreviewStates.claudeSession.isBoxed, "an open session")
  }
}
