import AppKit
import AtlasServiceTesting
import AtlasUI
import CoreGraphics
import Testing

@testable import AtlasApp

@Suite("Console session availability", .serialized, AppKitState())
@MainActor
struct SessionConsoleTests {
  @Test("only fresh on-console evidence for this login permits switching accounts")
  func theCurrentSessionMustOwnTheConsole() {
    let uid: uid_t = 501
    let user = kCGSessionUserIDKey as String
    let console = kCGSessionOnConsoleKey as String
    #expect(!SessionConsole.permitsSwitch(session: nil, uid: uid))
    #expect(!SessionConsole.permitsSwitch(session: [:], uid: uid))
    #expect(!SessionConsole.permitsSwitch(session: [user: uid, console: false], uid: uid))
    #expect(!SessionConsole.permitsSwitch(session: [user: uid + 1, console: true], uid: uid))
    #expect(SessionConsole.permitsSwitch(session: [user: NSNumber(value: uid), console: true], uid: uid))
  }

  @Test("every setup footer and helper menu disables switching while this session is off-console")
  func allSwitchOfferingsFollowConsoleOwnership() throws {
    for onConsole in [false, true] {
      var setup = OnboardingSnapshot()
      setup.isOnConsole = onConsole
      #expect(OnboardingPresenter.footer(setup, step: .helperDone).next?.isEnabled == onConsole)
      #expect(OnboardingPresenter.footer(setup, step: .switchToHelper).next?.isEnabled == onConsole)
      var viewer = ViewerSnapshot()
      viewer.isOnConsole = onConsole
      #expect(viewer.helperMenu.canSwitch == onConsole)
      var settings = SettingsSnapshot()
      settings.facts.isOnConsole = onConsole
      #expect(SettingsPresenter.helper(settings).buttons.canSwitch == onConsole)
      var panel = PanelSnapshot()
      panel.isOnConsole = onConsole
      panel.status = try status(["overview": ["mode": "loggedOut"]])
      #expect(PanelPresenter.main(panel, mode: PanelPresenter.mode(panel)).isEnabled == onConsole)
      let alert = AlertPresenter.state(AlertSnapshot(alert: .switchToHelper, isOnConsole: onConsole))
      #expect(alert.buttons.contains { $0.id == .confirm } == onConsole)
    }
  }

  @Test("an off-console startup notice offers only OK so Return cannot switch accounts")
  func noticeReturnCannotSwitchOffConsole() {
    let hidden = NoticeAlert.make(title: "Ready", message: "Ready", switchBack: "person", onConsole: false)
    #expect(hidden.buttons.map(\.title) == ["OK"])
    let visible = NoticeAlert.make(title: "Ready", message: "Ready", switchBack: "person", onConsole: true)
    #expect(visible.buttons.map(\.title) == ["Switch Back", "Stay Here"])
    #expect(visible.buttons.first?.keyEquivalent == "\r")
  }
}
