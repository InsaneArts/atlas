import AtlasService
import Testing

@testable import AtlasStatus

@Suite("IdleTimeNote")
struct IdleTimeNoteTests {
  /// A status saying how the person's idle time stands.
  ///
  /// - Parameters:
  ///   - state: How it stands.
  ///   - app: The app keeping it stopped.
  /// - Returns: The status.
  private func status(_ state: ViewerStatus.IdleTime.State, app: String? = nil) -> ViewerStatus {
    var status = ViewerStatus()
    status.idleTime = ViewerStatus.IdleTime(state: state, app: app)
    return status
  }

  @Test("an idle time an app keeps stopped shows a note naming the app")
  func namesTheAppToQuit() {
    #expect(IdleTimeNote.shown(status(.held, app: "Terminal"), dismissed: false) == IdleTimeNote(app: "Terminal"))
  }

  @Test("an app to quit with no name, or an empty one, is quit without a name")
  func quitsAnUnnamedApp() {
    for app in [nil, ""] {
      #expect(IdleTimeNote.shown(status(.held, app: app), dismissed: false) == IdleTimeNote(app: nil))
    }
  }

  @Test("the note goes once the person says OK")
  func hiddenOnceDismissed() {
    #expect(IdleTimeNote.shown(status(.held, app: "Terminal"), dismissed: true) == nil)
  }

  @Test("the note never shows while the idle time is live, unknown, or stuck with no app to quit")
  func hiddenOtherwise() {
    for state in [ViewerStatus.IdleTime.State.live, .unknown, .stuck] {
      #expect(IdleTimeNote.shown(status(state, app: "Terminal"), dismissed: false) == nil)
    }
  }
}
