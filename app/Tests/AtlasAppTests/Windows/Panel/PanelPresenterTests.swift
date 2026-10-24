import AtlasServiceTesting
import AtlasUI
import CoreGraphics
import Testing

@testable import AtlasApp

@Suite("PanelPresenter")
struct PanelPresenterTests {
  /// What the controller holds for one of the status table's rows, by its word: the service's mode and state for it,
  /// whether Atlas's main window holds control, and whether the panel is still connecting.
  ///
  /// - Parameter name: The row's word, such as "Needs you".
  /// - Returns: The snapshot.
  /// - Throws: A decoding error, which a fixed reply never gives, or when the row is not one of the table's.
  private static func snapshot(_ name: String) throws -> PanelSnapshot {
    let facts: [String: (mode: String, state: String, holding: Bool)] = [
      "Idle": ("observing", "idle", false),
      "Working": ("agent", "working", false),
      "You have control": ("controlling", "control", true),
      "Needs you": ("recovering", "needsYou", false),
      "Stopped": ("stopped", "stopped", false),
      "Logged out": ("loggedOut", "notReady", false),
      "Not ready": ("unavailable", "notReady", false),
      "Connecting": ("observing", "idle", false),
      "In use elsewhere": ("controlling", "control", false),
    ]
    let row = try #require(facts[name], "\(name) is not a row of the status table")
    return PanelSnapshot(
      status: try status(["overview": ["mode": row.mode, "state": row.state]]),
      connecting: name == "Connecting",
      holdingControl: row.holding
    )
  }

  @Test(
    "each state offers its one main action",
    arguments: [
      ("Idle", "Open Atlas", PanelEvent.openAtlas),
      ("Working", "Open Atlas", .openAtlas),
      ("You have control", "Give Back", .giveBack),
      ("Needs you", "Recover…", .askRecover),
      ("Stopped", "Resume", .resumeAgent),
      ("Logged out", "Switch to the Helper…", .askSwitch),
      ("Not ready", "Recover…", .askRecover),
      ("Connecting", "Open Atlas", .openAtlas),
      ("In use elsewhere", "Open Atlas", .openAtlas),
    ]
  )
  func eachStateOffersItsMainAction(row: String, title: String, event: PanelEvent) throws {
    let panel = PanelPresenter.state(try Self.snapshot(row))
    #expect(panel.status.label == row, "the facts read as \(row)")
    #expect(panel.controls.main.title == title)
    #expect(panel.controls.main.event == event)
    // Connecting waits, and so does Recover… for a helper that isn't answering, which Recover cannot fix.
    #expect(panel.controls.main.isEnabled == (row != "Connecting" && row != "Not ready"))
    #expect(panel.controls.showsStop == (row == "Working"), "Stop stands before it only while an agent works")
  }

  @Test("a main action that asks for one of the helper's commands waits while that command runs from any window")
  func theMainActionWaitsForItsCommand() throws {
    var needsYou = try Self.snapshot("Needs you")
    needsYou.running = [.switchToHelper]
    #expect(PanelPresenter.state(needsYou).controls.main.isEnabled, "another command does not hold it")
    needsYou.running = [.recovery]
    #expect(!PanelPresenter.state(needsYou).controls.main.isEnabled)
    var loggedOut = try Self.snapshot("Logged out")
    loggedOut.running = [.switchToHelper]
    #expect(!PanelPresenter.state(loggedOut).controls.main.isEnabled)
  }

  @Test(
    "the picture wears the rim in control, dims while the helper needs the person, fades while paused, and gives way to why while logged out or not ready, with Recent"
  )
  func thePictureFollowsTheState() throws {
    func picture(_ row: String, paused: Bool = false) throws -> (PanelPictureState, Bool) {
      var snapshot = try Self.snapshot(row)
      snapshot.session.paused = paused
      let state = PanelPresenter.state(snapshot)
      return (state.picture, state.recent.isEmpty)
    }
    #expect(try picture("You have control", paused: true).0 == .screen(picture: nil, look: .inControl, caption: nil))
    #expect(try picture("Needs you").0 == .screen(picture: nil, look: .dimmed, caption: nil))
    #expect(try picture("Idle", paused: true).0 == .screen(picture: nil, look: .paused, caption: nil))
    #expect(try picture("Logged out") == (.empty(PanelWords.loggedOutPicture), true))
    #expect(try picture("Not ready") == (.empty(PanelWords.notReadyWaitPicture), true), "Recover cannot fix it")
  }

  @Test("while an agent works the picture carries its newest action alone, and none once paused")
  func theCaptionIsTheNewestActionAlone() throws {
    var snapshot = try Self.snapshot("Working")
    snapshot.status.overview.activity = [
      .init(operation: "input", ok: true, ageSeconds: 2, says: "clicked Save"),
      .init(operation: "input", ok: true, ageSeconds: 9, says: "opened TextEdit"),
    ]
    guard case .screen(_, _, let caption) = PanelPresenter.state(snapshot).picture else {
      Issue.record("a working helper shows its screen")
      return
    }
    #expect(caption?.line == "Clicked Save")
    snapshot.session.paused = true
    #expect(PanelPresenter.state(snapshot).picture == .screen(picture: nil, look: .paused, caption: nil))
  }

  @Test(
    "Recover… is offered only where it fixes something, by the main window's rule: an unknown outcome or input it starts"
  )
  func recoverIsOfferedOnlyWhereItHelps() throws {
    #expect(!PanelPresenter.moreMenu(try Self.snapshot("Idle")).helper.canRecover)
    #expect(PanelPresenter.moreMenu(try Self.snapshot("Needs you")).helper.canRecover)
    #expect(!PanelPresenter.moreMenu(try Self.snapshot("Not ready")).helper.canRecover, "an unreachable service")
    let blocked = { (recoverable: Bool) throws -> PanelSnapshot in
      PanelSnapshot(
        status: try status([
          "overview": ["mode": "blocked", "state": recoverable ? "needsYou" : "notReady", "recoverable": recoverable]
        ])
      )
    }
    #expect(PanelPresenter.moreMenu(try blocked(true)).helper.canRecover, "input Recover starts again")
    #expect(!PanelPresenter.moreMenu(try blocked(false)).helper.canRecover, "a block Recover can't fix")
  }

  @Test("Open at Login shows in the ⋯ menu only when the app carries the login item")
  func openAtLogin() throws {
    var snapshot = try Self.snapshot("Idle")
    #expect(PanelPresenter.moreMenu(snapshot).opensAtLogin == nil)
    snapshot.opensAtLogin = OpenAtLogin.State(isAvailable: false, isEnabled: false)
    #expect(PanelPresenter.moreMenu(snapshot).opensAtLogin == nil)
    snapshot.opensAtLogin = OpenAtLogin.State(isAvailable: true, isEnabled: true)
    #expect(PanelPresenter.moreMenu(snapshot).opensAtLogin == true)
  }
}
