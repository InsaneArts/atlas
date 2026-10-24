import AtlasCommands
import AtlasService
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("SettingsPresenter")
struct SettingsPresenterTests {
  @Test("Paused is available only with an open viewer or an already paused picture", arguments: [false, true])
  func pausedAvailability(canPause: Bool) throws {
    for picture in [ViewerPictureChoice.live, .everyFiveSeconds, .paused] {
      var snapshot = SettingsSnapshot()
      snapshot.facts.canPause = canPause
      snapshot.facts.picture = picture
      let general = SettingsPresenter.general(snapshot)
      #expect(general.pictureOptions.map(\.id) == [.live, .everyFiveSeconds, .paused])
      #expect(general.pictureOptions.map(\.title) == ["Live", "Every 5 Seconds", "Paused"])
      #expect(general.pictureOptions[2].isEnabled == (canPause || picture == .paused))
      #expect(general.picture == SettingsPresenter.picture(picture))
    }
  }

  @Test("startup preserves the reported unavailable state and Notifications stays off by default")
  func unavailableStartupAndNotificationDefault() {
    var snapshot = SettingsSnapshot()
    snapshot.facts.opensAtLogin = OpenAtLogin.State(isAvailable: false, isEnabled: false)
    let general = SettingsPresenter.general(snapshot)
    #expect(!general.canOpenAtLogin && !general.opensAtLogin)
    #expect(!general.notify)
  }

  @Test("readiness summaries distinguish empty, unknown, mixed, ready and explicitly checking rows")
  func readinessSummaries() {
    var helper = SettingsPresenter.helper(SettingsSnapshot())
    helper.ready = []
    #expect(helper.readinessSummary == "Not checked yet")
    helper.ready = SettingsPresenter.ready([ReadinessItem(name: "Future check", text: "Waiting", state: .unknown)])
    #expect(helper.readinessSummary == "1 not checked")
    helper.ready += SettingsPresenter.ready([
      ReadinessItem(name: "Permission", text: "Use macOS settings", state: .notReady),
      ReadinessItem(name: "Native dialogs", text: "Ready", state: .ready),
    ])
    #expect(helper.readinessSummary == "1 needing attention · 1 not checked")
    #expect(helper.ready.map(\.item.name) == ["Future check", "Permission", "Native dialogs"])
    #expect(helper.ready.first?.item.text == "Waiting")
    helper.ready = SettingsPresenter.ready([ReadinessItem(name: "Future check", text: "", state: .ready)])
    #expect(helper.readinessSummary == "All reported checks passed")
    helper.buttons.canCheck = false
    #expect(helper.readinessSummary == "Checking…")
  }

  @Test("project summaries count loaded paths and retain missing sections and duplicate folder names")
  func projectSummariesAndIdentity() throws {
    var agents = SettingsPresenter.agents(SettingsSnapshot())
    agents.projectsReading = .loaded
    agents.projects = []
    #expect(agents.projectsSummary == "No projects taught")
    let report = try JSONDecoder()
      .decode(
        AgentsStatus.self,
        from: Data(
          #"{"ok":true,"agents":[],"projects":[{"path":"/work/same","taught":true},{"path":"/archive/same","taught":false}]}"#
            .utf8
        )
      )
    let projects = try #require(report.projects)
    let first = SettingsPresenter.project(projects[0])
    let second = SettingsPresenter.project(projects[1])
    agents.projects = [first]
    #expect(agents.projectsSummary == "1 project taught")
    agents.projects.append(second)
    #expect(agents.projectsSummary == "2 projects taught")
    #expect(first.name == second.name && first.id != second.id)
    #expect(second.detail == "No longer has the Atlas section" && !second.detailIsPath)
  }

  @Test("About shows one control for updates, whatever the update state, and what runs under it, not in it")
  func aboutShowsOneControlForUpdates() {
    let misplaced =
      "Atlas.app must be where the helper account can run it: install it in /Applications, then check for updates "
      + "again. The release was downloaded and its signature checked, but nothing was replaced."
    let cases: [(SettingsUpdate, SettingsUpdateButton, SettingsUpdateStatus?)] = [
      (.idle, .check, nil),
      (.checking, .checking, .working("Checking for updates…")),
      (.answered(.upToDate), .check, .done("Atlas is up to date.")),
      (.answered(.newer("0.2.0")), .install, .available("Atlas 0.2.0 is available.")),
      (.answered(.failed), .check, .problem("Atlas couldn't check for updates.")),
      (.answered(.unpublished), .check, .note(SettingsWords.unpublished)),
      (.installing("0.2.0"), .installing, .installing("Installing Atlas 0.2.0…")),
      (.installed, .check, .done(SettingsWords.installed)),
      (.installFailed("The download failed."), .check, .problem("The download failed.")),
      (.installFailed(misplaced), .check, .misplaced("Move Atlas to the Applications folder, then try again.")),
    ]
    for (update, button, status) in cases {
      var snapshot = SettingsSnapshot()
      snapshot.session.update = update
      let about = SettingsPresenter.about(snapshot)
      #expect(about.update == button, "\(update)")
      #expect(about.status == status, "\(update)")
    }
  }

  @Test("a deleted account's open login comes first with Close It…, then a silent helper's with Log Out…, else none")
  func aLoginAtlasCanCloseIsListed() {
    var snapshot = SettingsSnapshot()
    #expect(SettingsPresenter.helper(snapshot).closeLogin == nil)

    snapshot.facts.status.helperStale = true
    let stale = SettingsPresenter.helper(snapshot).closeLogin
    #expect(stale?.title == SettingsWords.staleTitle)
    #expect(stale?.button == "Log Out…")
    #expect(stale?.event == .closeHelper)

    snapshot.facts.status.leftoverLogins = [502]
    let leftover = SettingsPresenter.helper(snapshot).closeLogin
    #expect(leftover?.title == "A deleted account is still logged in")
    #expect(leftover?.button == "Close It…")
    #expect(leftover?.event == .closeLeftovers)
    #expect(leftover?.canRun == true)
    #expect(leftover?.status == nil)
  }

  @Test("Close It… waits, saying so, while it closes, and while another action holds the pane")
  func closeItWaits() {
    var snapshot = SettingsSnapshot()
    snapshot.facts.status.leftoverLogins = [502]
    snapshot.session.closing = true
    let closing = SettingsPresenter.helper(snapshot).closeLogin
    #expect(closing?.canRun == false)
    #expect(closing?.status == "Closing…")

    snapshot.session.closing = false
    snapshot.facts.running = [.logOut]
    #expect(SettingsPresenter.helper(snapshot).closeLogin?.canRun == false, "another window's Log Out runs")
  }

  @Test("Check Again shows a spinner in every check's mark while it reads")
  func checkAgainSpinsEveryMark() {
    let items = [
      ReadinessItem(name: "Helper is running", text: "", state: .ready),
      ReadinessItem(name: "Mouse and keyboard", text: "Recover starts them again.", state: .notReady),
    ]
    #expect(SettingsPresenter.ready(items, checking: true).allSatisfy { $0.isChecking })
    #expect(!SettingsPresenter.ready(items).contains { $0.isChecking })
  }

  @Test("a failing check Recover fixes offers Recover… in its own row, with its line cut short; others don't")
  func aFailingCheckOffersRecover() {
    let items = [
      ReadinessItem(name: "Screen Recording", text: "Turn on Atlas in the helper's settings.", state: .notReady),
      ReadinessItem(name: "Mouse and keyboard", text: "Mouse and keyboard stopped in the helper.", state: .notReady),
    ]
    let rows = SettingsPresenter.ready(items, recoverLines: ["Mouse and keyboard": "Stopped in the helper."])
    #expect(rows.map(\.offersRecover) == [false, true])
    #expect(rows[1].item.text == "Stopped in the helper.")
    #expect(rows[0].item.text == "Turn on Atlas in the helper's settings.", "a fix Recover can't make keeps its words")
  }

  @Test("Recover's short line is for input that stopped or isn't ready, and never for a permission")
  func recoverLinesAreForInputOnly() {
    func check(_ name: String, step: String) -> ViewerStatus.Check {
      ViewerStatus.Check(name: name, ok: false, nextStep: step)
    }
    #expect(CheckWords.recoverLine(check("Input service ready", step: "atlas start-input")) == "Stopped in the helper.")
    #expect(CheckWords.recoverLine(check("Input service ready", step: "")) == "Not ready in the helper.")
    #expect(CheckWords.recoverLine(check("Input service ready", step: "Turn on Accessibility")) == nil)
    #expect(CheckWords.recoverLine(check("Screen Recording allowed", step: "")) == nil)
  }

  @Test("while another window's command runs, the pane names it")
  func theWaitingLineNamesTheCommand() {
    #expect(HelperWords.running([.recovery]) == "Recover is running. Try again once it ends.")
    #expect(HelperWords.running([.logOut]) == "Log Out is running. Try again once it ends.")
    #expect(HelperWords.running([.switchToHelper, .recovery]).hasPrefix("Recover"), "Recover names first")
  }

  @Test("what Check Again found shows under Ready to act, and every other notice under the last section")
  func checkNoticesShowUnderReadyToAct() {
    var snapshot = SettingsSnapshot()
    snapshot.session.notice = SettingsNotice(tab: .helper, text: "Not ready.", kind: .problem, fromCheck: true)
    #expect(SettingsPresenter.helper(snapshot).readyNote?.text == "Not ready.")
    #expect(SettingsPresenter.helper(snapshot).notice == nil)
    snapshot.session.notice = SettingsNotice(tab: .helper, text: "The helper is logged out.")
    #expect(SettingsPresenter.helper(snapshot).readyNote == nil)
    #expect(SettingsPresenter.helper(snapshot).notice?.text == "The helper is logged out.")
  }

  @Test("each readiness row carries its place in the list, which staggers its mark's pop, checking or not")
  func eachRowCarriesItsPlace() {
    let items = ["Helper is running", "Mouse and keyboard", "Screen pictures"]
      .map { ReadinessItem(name: $0, text: "", state: .ready) }
    #expect(SettingsPresenter.ready(items).map(\.index) == [0, 1, 2])
    #expect(SettingsPresenter.ready(items, checking: true).map(\.index) == [0, 1, 2])
  }
}
