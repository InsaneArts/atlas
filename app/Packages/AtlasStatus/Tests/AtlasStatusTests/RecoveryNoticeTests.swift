import AtlasService
import Testing

@testable import AtlasStatus

@Suite("RecoveryNotice")
struct RecoveryNoticeTests {
  /// An overview in `mode` and `state`, as the service files them.
  ///
  /// - Parameters:
  ///   - mode: The helper's mode.
  ///   - state: What every agent app shows.
  /// - Returns: The overview.
  private func overview(_ mode: HelperMode, _ state: OverviewState) -> ViewerStatus.Overview {
    var overview = ViewerStatus.Overview()
    overview.mode = mode
    overview.state = state
    return overview
  }

  /// What each answer tells, after the one before it, starting from launch.
  ///
  /// - Parameter answers: The service's answers in turn.
  /// - Returns: Each answer's notice.
  private func notices(_ answers: [ViewerStatus.Overview]) -> [RecoveryNotice?] {
    answers.indices.map { index in
      RecoveryNotice.of(
        earlier: index > 1 ? answers[index - 2] : nil,
        previous: index > 0 ? answers[index - 1] : nil,
        current: answers[index]
      )
    }
  }

  @Test("a notification is sent once per new recovery, not on every poll")
  func oncePerRecovery() {
    let idle = overview(.observing, .idle)
    let unknown = overview(.recovering, .needsYou)
    let inputStopped = overview(.blocked, .needsYou)
    // An unknown outcome is told at once; an action left unknown after input stopped is a new one. Input stopping
    // after an unknown outcome is the same recovery, half done.
    let told = notices([idle, unknown, unknown, unknown, idle, inputStopped, unknown, unknown, inputStopped, unknown])
    #expect(told == [nil, .unfinished, nil, nil, nil, nil, .unfinished, nil, nil, .unfinished])
  }

  @Test("input that stops is told once it lasts across two readings, never for the moment a helper logs in")
  func inputStoppedOnceItLasts() {
    let idle = overview(.observing, .idle)
    let inputStopped = overview(.blocked, .needsYou)
    #expect(notices([idle, inputStopped, idle]) == [nil, nil, nil], "input started by itself after login")
    #expect(notices([idle, inputStopped, inputStopped, inputStopped]) == [nil, nil, .inputStopped, nil])
    #expect(!RecoveryNotice.needsPerson(previous: idle, current: inputStopped), "no badge for the moment")
    #expect(RecoveryNotice.needsPerson(previous: inputStopped, current: inputStopped))
    #expect(RecoveryNotice.needsPerson(previous: idle, current: overview(.recovering, .needsYou)))
  }

  @Test("a helper logging out is told once, and a state already there at launch is not told")
  func loggedOutOnce() {
    let loggedOut = overview(.loggedOut, .notReady)
    let needsYou = overview(.recovering, .needsYou)
    let idle = overview(.observing, .idle)
    #expect(notices([loggedOut, loggedOut, idle, loggedOut, loggedOut]) == [nil, nil, nil, .loggedOut, nil])
    #expect(notices([needsYou, needsYou]) == [nil, nil], "Needs you at launch was there before Atlas started")
  }
}
