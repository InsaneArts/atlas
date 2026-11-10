import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Settings navigation")
@MainActor
struct SettingsNavigationTests {
  /// Lets queued work run until `commands` has run `action`, or a bound of turns passes.
  ///
  /// - Parameters:
  ///   - commands: The scripted command line.
  ///   - action: The command waited for.
  /// - Returns: Whether it ran.
  private static func ran(_ commands: ScriptedCommands, _ action: Action) async -> Bool {
    for _ in 0..<20 where !commands.ran.contains(action.arguments) {
      await afterQueuedWork()
    }
    return commands.ran.contains(action.arguments)
  }

  @Test("a page chosen in the sidebar shows, Back returns and Forward goes again, and the bar's title follows")
  func theSidebarAndBackAndForward() {
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]))
    #expect(controller.state.route == .page(.helper), "Helper first, with nothing saved")
    controller.send(.select(.general))
    controller.send(.select(.about))
    #expect(controller.state.title == "About")
    controller.send(.goBack)
    #expect(controller.state.route == .page(.general))
    #expect(controller.state.navigation.canGoForward)
    controller.send(.goForward)
    #expect(controller.state.route == .page(.about))
  }

  @Test("each page shown is saved for the next time the window opens, an account's page as Accounts")
  func thePageShownIsSaved() {
    var saved: [String] = []
    let controller = SettingsController.scripted(
      commands: ScriptedCommands([:]),
      edit: { dependencies in
        dependencies.preferences.saveTab = { saved.append($0) }
      }
    )
    controller.send(.select(.privacy))
    controller.send(.openAccount("github"))
    controller.send(.goBack)
    #expect(saved == ["privacy", "accounts", "privacy"])
  }

  @Test("the saved page comes back, the old Agents tab as Integrations, and anything unknown as Helper")
  func theSavedPageComesBack() {
    for (saved, page) in [
      ("privacy", SettingsPage.privacy), ("agents", .integrations), ("general", .general), ("tabs", .helper),
    ] {
      let controller = SettingsController.scripted(
        commands: ScriptedCommands([:]),
        edit: { dependencies in
          dependencies.preferences.loadTab = { saved }
        }
      )
      #expect(controller.state.route == .page(page), "\(saved)")
    }
    #expect(SettingsPage.saved(nil) == .helper)
  }

  @Test("Privacy reads the person's files whichever way the window comes to it")
  func privacyReadsItsFilesOnEveryWayIn() async {
    let sidebar = ScriptedCommands([:])
    SettingsController.scripted(commands: sidebar)
      .send(.select(.privacy))
    #expect(await Self.ran(sidebar, .homeFiles), "from the sidebar")

    let back = ScriptedCommands([:])
    let walking = SettingsController.scripted(commands: back)
    walking.send(.select(.privacy))
    walking.send(.select(.about))
    await afterQueuedWork()
    let before = back.ran.count { $0 == Action.homeFiles.arguments }
    walking.send(.goBack)
    for _ in 0..<20 where back.ran.count(where: { $0 == Action.homeFiles.arguments }) == before {
      await afterQueuedWork()
    }
    #expect(back.ran.count { $0 == Action.homeFiles.arguments } > before, "by Back")

    let restored = ScriptedCommands([:])
    let opened = SettingsController.scripted(
      commands: restored,
      edit: { dependencies in
        dependencies.preferences.loadTab = { "privacy" }
      }
    )
    opened.cameForward()
    #expect(await Self.ran(restored, .homeFiles), "restored as the window opens")
  }

  @Test("the Helper row says how the helper stands, as the Helper page's card does")
  func theHelperRowFollowsTheHelper() {
    var facts = SettingsFacts()
    facts.connecting = true
    #expect(SettingsPresenter.helperStanding(facts).word == "Connecting")
    #expect(SettingsPresenter.standing(facts).tone == .quiet)
    var snapshot = SettingsSnapshot()
    snapshot.facts = facts
    let row = SettingsPresenter.sidebar(snapshot).rows.first
    #expect(row?.page == .helper)
    #expect(row?.subtitle == "Connecting")
    #expect(SettingsPresenter.helper(snapshot).account.status.label == "Connecting")
  }
}
