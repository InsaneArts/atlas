import Testing

@testable import AtlasKit

@Suite("Mode")
struct ModeTests {
  @Test("an overview state the app doesn't know reads as not ready, and a logged-out helper as logged out")
  func unknownAndLoggedOut() throws {
    let unknown = try status(["overview": ["mode": "observing", "state": "dreaming", "label": "Dreaming"]])
    #expect(unknown.overview.state == .unknown("dreaming"))
    #expect(Mode.of(unknown.overview) == Mode(.notReady), "the unknown state's own word is not shown")
    let loggedOut = try status(["overview": ["mode": "loggedOut", "state": "notReady", "label": "Not ready"]])
    #expect(Mode.of(loggedOut.overview).kind == .loggedOut, "the service files it under not ready")
  }

  @Test("a state keeps the service's word, and none when the service gives none or an empty one")
  func serviceWord() throws {
    let worded = try status(["overview": ["mode": "agent", "state": "working", "label": "Busy"]])
    #expect(Mode.of(worded.overview) == Mode(.working, label: "Busy"))
    let bare = try status(["overview": ["mode": "agent", "state": "working"]])
    #expect(Mode.of(bare.overview).label == nil)
    #expect(Mode(.working, label: "").label == nil)
  }

  @Test(
    "a viewer tells its own control from another window's, and never shows control as yours while recovering or logged out"
  )
  func seen() {
    var overview = ViewerStatus.Overview()
    overview.mode = .controlling
    overview.state = .control
    #expect(Mode.seen(by: overview, holdingControl: true).kind == .control)
    #expect(Mode.seen(by: overview, holdingControl: false).kind == .inUseElsewhere)
    overview.mode = .recovering
    overview.state = .needsYou
    #expect(Mode.seen(by: overview, holdingControl: true).kind == .needsYou)
    overview.mode = .loggedOut
    overview.state = .notReady
    #expect(Mode.seen(by: overview, holdingControl: true).kind == .loggedOut, "no control without a helper")
  }
}
