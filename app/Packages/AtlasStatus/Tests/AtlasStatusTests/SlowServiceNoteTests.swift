import AtlasService
import Testing

@testable import AtlasStatus

@Suite("SlowServiceNote")
struct SlowServiceNoteTests {
  /// A status saying how the helper's restarting service stands.
  ///
  /// - Parameter restarting: How it stands.
  /// - Returns: The status.
  private func status(_ restarting: ViewerStatus.RestartingService) -> ViewerStatus {
    var status = ViewerStatus()
    status.restartingService = restarting
    return status
  }

  @Test("the note shows while the service restarts in the helper, until the person says OK")
  func showsUntilDismissed() {
    #expect(SlowServiceNote.shows(status(.restarting), dismissed: false))
    #expect(!SlowServiceNote.shows(status(.restarting), dismissed: true))
  }

  @Test("the note never shows while nothing restarts or Atlas could not tell")
  func hiddenOtherwise() {
    for restarting in [ViewerStatus.RestartingService.clear, .unknown] {
      #expect(!SlowServiceNote.shows(status(restarting), dismissed: false))
    }
  }
}
