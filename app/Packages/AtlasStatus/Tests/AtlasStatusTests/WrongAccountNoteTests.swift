import AtlasService
import Testing

@testable import AtlasStatus

@Suite("WrongAccountNote")
struct WrongAccountNoteTests {
  @Test("only another named account shows until the person dismisses it")
  func onlyAnotherNamedAccountShowsUntilDismissed() {
    for state in [ViewerStatus.ConsoleName.State.this, .another, .unknown] {
      for name: String? in [nil, "", "atlas"] {
        for dismissed in [false, true] {
          var status = ViewerStatus()
          status.consoleName = ViewerStatus.ConsoleName(state: state, name: name)
          let expected = state == .another && name == "atlas" && !dismissed ? "atlas" : nil
          #expect(WrongAccountNote.name(status, dismissed: dismissed) == expected)
        }
      }
    }
  }
}
