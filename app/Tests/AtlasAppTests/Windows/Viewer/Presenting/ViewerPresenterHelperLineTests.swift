import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerPresenter+HelperLine")
struct ViewerPresenterHelperLineTests {
  /// A viewer that has read a status in `mode` and `state`, for the helper `atlas` on a 1440 by 900 display.
  ///
  /// - Parameters:
  ///   - mode: The overview's mode.
  ///   - state: The overview's state.
  ///   - checks: The readiness checks; none by default.
  /// - Returns: The snapshot.
  /// - Throws: When the status does not decode.
  static func reading(_ mode: String, _ state: String, checks: [[String: Any]] = []) throws -> ViewerSnapshot {
    var snapshot = ViewerSnapshot.presenterSample
    snapshot.reading.status = try status([
      "checks": checks,
      "overview": [
        "mode": mode, "state": state, "account": "atlas", "display": ["width": 1440, "height": 900],
      ],
    ])
    snapshot.reading.hasStatus = true
    snapshot.reading.ready = true
    snapshot.reading.frame = (id: "f", revision: "r", display: ViewerStatus.Size(width: 4, height: 3))
    snapshot.reading.frameAt = 0
    return snapshot
  }

  @Test("a ready helper says Atlas is ready over its name and display, in green")
  func aReadyHelperSaysItIsReady() throws {
    let line = ViewerPresenter.helperLine(try Self.reading("observing", "idle"))
    #expect(line.tone == .ready)
    #expect(line.title == "Atlas is ready")
    #expect(line.line == "Helper atlas · 1440 × 900")
    #expect(line.lineTone == .quiet)
  }

  @Test("a check that does not pass says Atlas needs setup and how many do not, in orange")
  func aFailingCheckSaysAtlasNeedsSetup() throws {
    let one = try Self.reading(
      "observing",
      "idle",
      checks: [["name": "Helper running", "ok": true], ["name": "Input service ready", "ok": false]]
    )
    let line = ViewerPresenter.helperLine(one)
    #expect(line.tone == .attention)
    #expect(line.title == "Atlas needs setup")
    #expect(line.line == "1 check doesn't pass")
    #expect(line.lineTone == .attention)
    let two = try Self.reading(
      "observing",
      "idle",
      checks: [["name": "Screen Recording", "ok": false], ["name": "Input service ready", "ok": false]]
    )
    #expect(ViewerPresenter.helperLine(two).line == "2 checks don't pass")
  }

  @Test("a helper that is not ready with no check failing still says Atlas needs setup, over its name in orange")
  func aHelperNotReadyWithNoFailingCheckNeedsSetup() throws {
    let line = ViewerPresenter.helperLine(try Self.reading("notReady", "notReady"))
    #expect(line.tone == .attention)
    #expect(line.title == "Atlas needs setup")
    #expect(line.line == "Helper atlas")
    #expect(line.lineTone == .attention)
  }

  @Test("a helper that needs the person says to recover it, in orange")
  func aHelperThatNeedsYouSaysToRecover() throws {
    let line = ViewerPresenter.helperLine(try Self.reading("recovering", "needsYou"))
    #expect(line.tone == .attention)
    #expect(line.title == "Atlas needs you")
    #expect(line.line == "Recover the helper to go on")
    #expect(line.lineTone == .attention)
  }

  @Test("a logged-out helper says to log in to it by its name, in orange")
  func aLoggedOutHelperSaysToLogIn() throws {
    var snapshot = try Self.reading("loggedOut", "notReady")
    snapshot.reading.picture = nil
    let line = ViewerPresenter.helperLine(snapshot)
    #expect(line.tone == .attention)
    #expect(line.title == "The helper is logged out")
    #expect(line.line == "Log in to atlas to go on")
    #expect(line.lineTone == .attention)
  }

  @Test("before the first reading the row says it connects, in grey, with the helper unnamed")
  func theFirstReadingConnects() {
    let line = ViewerPresenter.helperLine(ViewerSnapshot.presenterSample)
    #expect(line.tone == .quiet)
    #expect(line.title == "Connecting to Atlas…")
    #expect(line.line == "Helper")
  }

  @Test("the row's menu lists every fact about the helper, read-only, before its commands")
  func theMenuLeadsWithTheFacts() throws {
    let menu = ViewerPresenter.helperLine(try Self.reading("observing", "idle")).menu
    let facts = try #require(menu.first)
    #expect(facts.items.allSatisfy { $0.isInfo && !$0.isEnabled }, "facts cannot be chosen")
    #expect(facts.items.contains { $0.title == "Display: 1440 × 900" })
    #expect(menu.dropFirst().first?.items.first?.command == .checkAgain, "the commands follow, as before")
  }

  @Test("the helper's words name it, its display and its failing checks")
  func theWordsNameTheHelper() {
    #expect(ViewerWords.helperNamed("atlas", display: "1440 × 900") == "Helper atlas · 1440 × 900")
    #expect(ViewerWords.helperNamed("atlas") == "Helper atlas")
    #expect(ViewerWords.helperNamed(nil) == "Helper")
    #expect(ViewerWords.checksFailing(1) == "1 check doesn't pass")
    #expect(ViewerWords.checksFailing(3) == "3 checks don't pass")
    #expect(ViewerWords.logInLine("atlas") == "Log in to atlas to go on")
    #expect(ViewerWords.logInLine(nil) == "Log in to the helper to go on")
  }
}
