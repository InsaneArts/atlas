import AtlasKit
import AtlasUI
import CoreGraphics
import Testing

@testable import AtlasApp

@Suite("PanelPresenter")
struct PanelPresenterTests {
  /// A snapshot in `mode`, with the helper's names and display unless said otherwise.
  private static func snapshot(
    _ mode: HelperMode,
    reason: String = "Ready for agents",
    known: Bool = true,
    working: Bool = false,
    helper: String? = "atlas",
    logOut: LogOutStage = .idle
  ) -> PanelSnapshot {
    var status = ViewerStatus()
    status.overview.mode = mode
    status.overview.reason = reason
    if known {
      status.overview.account = "atlas"
      status.overview.accountName = "Atlas"
      status.overview.display = ViewerStatus.Size(width: 1512, height: 982)
    }
    return PanelSnapshot(
      status: status,
      feedback: working ? Words.recovering : nil,
      working: working,
      helper: helper,
      logOutStage: logOut
    )
  }

  @Test("a logged-out helper shows Log In in place of the picture and hides Switch, Recover and Log Out")
  func loggedOut() throws {
    let context = try #require(
      CGContext(
        data: nil,
        width: 1,
        height: 1,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
      )
    )
    var snapshot = Self.snapshot(.loggedOut)
    snapshot.picture = DesktopPicture(id: "f", image: try #require(context.makeImage()))
    let state = PanelPresenter.state(snapshot)
    #expect(state.stage == .loggedOut(text: Words.loggedOut, isEnabled: true))
    #expect(state.secondary == nil)
    #expect(state.menu.recover == .hidden)
    #expect(state.menu.logOut == .hidden)
    let working = PanelPresenter.state(Self.snapshot(.loggedOut, working: true))
    #expect(working.stage == .loggedOut(text: Words.loggedOut, isEnabled: false), "Log In… waits for a command")
  }

  @Test("Open Viewer is the primary button unless the helper is logged out")
  func openViewerLook() {
    #expect(PanelPresenter.state(Self.snapshot(.observing)).isViewerPrimary)
    #expect(!PanelPresenter.state(Self.snapshot(.loggedOut)).isViewerPrimary)
  }

  @Test("in control the second button releases control; otherwise Switch waits for a known helper and an idle panel")
  func secondaryButton() {
    #expect(PanelPresenter.state(Self.snapshot(.controlling, helper: nil)).secondary == .revokeControl)
    #expect(PanelPresenter.state(Self.snapshot(.agent)).secondary == .switchToHelper(isEnabled: true))
    #expect(PanelPresenter.state(Self.snapshot(.agent, helper: nil)).secondary == .switchToHelper(isEnabled: false))
    #expect(PanelPresenter.state(Self.snapshot(.agent, working: true)).secondary == .switchToHelper(isEnabled: false))
  }

  @Test(
    "the reason line hides when empty, recovering or logged out",
    arguments: [
      (HelperMode.observing, "Ready for agents", "Ready for agents"),
      (.observing, "", nil),
      (.recovering, "An action's outcome is unknown", nil),
      (.loggedOut, "The helper is logged out", nil),
    ] as [(HelperMode, String, String?)]
  )
  func reasonLine(mode: HelperMode, reason: String, shown: String?) {
    #expect(PanelPresenter.state(Self.snapshot(mode, reason: reason)).reason == shown)
  }

  @Test("a recovering helper shows the recovery card with its reason, or a fallback when it gives none")
  func recoveryCard() {
    #expect(PanelPresenter.state(Self.snapshot(.observing)).recovery == nil)
    #expect(
      PanelPresenter.state(Self.snapshot(.recovering, reason: "Unknown outcome")).recovery
        == .unfinished("Unknown outcome")
    )
    let fallback = PanelPresenter.state(Self.snapshot(.recovering, reason: "")).recovery
    #expect(fallback == .unfinished("Recover before anything else runs."))
  }

  @Test("input Recover starts again shows the not-ready card in place of the reason and the Recover row")
  func inputStoppedOffersRecover() {
    var stopped = Self.snapshot(
      .blocked,
      reason: "Mouse and keyboard stopped in the helper. Recover starts them again."
    )
    stopped.status.overview.recoverable = true
    let state = PanelPresenter.state(stopped)
    #expect(state.recovery == .notReady("Mouse and keyboard stopped in the helper. Recover starts them again."))
    #expect(state.reason == nil)
    #expect(state.menu.recover == .hidden)
    let settings = PanelPresenter.state(Self.snapshot(.blocked, reason: "Turn on Accessibility."))
    #expect(settings.recovery == nil)
    #expect(settings.reason == "Turn on Accessibility.")
    #expect(settings.menu.recover == .enabled)
  }

  @Test("the facts read a dash for an unknown helper and display")
  func factsFallBackToADash() {
    let known = PanelPresenter.state(Self.snapshot(.observing)).facts
    #expect(known.map(\.value) == ["Atlas", "1512 × 982", "No recent actions"])
    let unknown = PanelPresenter.state(Self.snapshot(.unavailable, known: false)).facts
    #expect(unknown.map(\.name) == ["Helper", "Display", "Last action"])
    #expect(unknown.map(\.value) == [Words.unknown, Words.unknown, "No recent actions"])
  }

  // Which modes allow a log-out, forced or not, is HelperMode's (HelperModeLogOutTests); these check one mode per
  // branch, so each step follows its own rule of the two, and waits while a command runs.
  @Test("the log-out confirmation follows whether a log-out is allowed, and Log Out Anyway whether forcing one is")
  func logOutSteps() {
    #expect(PanelPresenter.state(Self.snapshot(.observing)).logOut == PanelLogOut.none)
    let asking = PanelPresenter.state(Self.snapshot(.observing, logOut: .confirming))
    #expect(asking.logOut == .confirming(isEnabled: true))
    #expect(asking.menu.logOut == .disabled, "Log Out Helper… waits while the person is asked")
    #expect(
      PanelPresenter.state(Self.snapshot(.recovering, logOut: .confirming)).logOut == .confirming(isEnabled: false),
      "recovering forbids a log-out"
    )
    #expect(
      PanelPresenter.state(Self.snapshot(.recovering, logOut: .held)).logOut == .held(isEnabled: true),
      "recovering still allows forcing one"
    )
    #expect(
      PanelPresenter.state(Self.snapshot(.observing, working: true, logOut: .confirming)).logOut
        == .confirming(isEnabled: false)
    )
    #expect(
      PanelPresenter.state(Self.snapshot(.recovering, working: true, logOut: .held)).logOut == .held(isEnabled: false)
    )
  }

  @Test("the feedback's spinner turns while any command runs, whichever action said the words")
  func theSpinnerFollowsWork() {
    var snapshot = Self.snapshot(.observing)
    snapshot.feedback = PanelWords.released
    #expect(PanelPresenter.state(snapshot).feedback == Feedback(text: PanelWords.released, isWorking: false))
    snapshot.working = true
    #expect(PanelPresenter.state(snapshot).feedback == Feedback(text: PanelWords.released, isWorking: true))
  }

  @Test("Open at Login shows only when the app carries the login item")
  func openAtLogin() {
    var snapshot = Self.snapshot(.observing)
    #expect(PanelPresenter.state(snapshot).menu.opensAtLogin == nil)
    snapshot.opensAtLogin = OpenAtLogin.State(isAvailable: false, isEnabled: false)
    #expect(PanelPresenter.state(snapshot).menu.opensAtLogin == nil)
    snapshot.opensAtLogin = OpenAtLogin.State(isAvailable: true, isEnabled: true)
    #expect(PanelPresenter.state(snapshot).menu.opensAtLogin == true)
  }
}
