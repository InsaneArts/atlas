import AtlasCommandsTesting
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Settings menu bar icon")
struct SettingsMenuBarIconTests {
  @Test("General shows the icon the menu bar draws, and picking another draws it once")
  func generalPicksTheIcon() {
    let icon = MenuBarIconChoice(.screen)
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]), menuBarIcon: icon)
    #expect(controller.state.general.menuBarIcon == .screen)
    var general = controller.state.general
    general.menuBarIcon = .orbit

    controller.update(general)

    #expect(icon.picks == [.orbit])
    #expect(controller.state.general.menuBarIcon == .orbit)
    controller.update(controller.state.general)
    #expect(icon.picks == [.orbit], "a pane with nothing changed picks nothing")
  }

  @Test("the icon row sits in General's first section, after Open Atlas at login")
  func theRowFollowsOpenAtLogin() {
    #expect(GeneralRow.start == [.openAtLogin, .menuBarIcon, .picture, .muteHelper])
  }

  @Test("Settings' links read and set the menu bar's icon through the app's parts")
  func theLinksReachTheMenuBar() {
    let parts = StandInParts()
    // Held, as the links reach the parts through it weakly.
    let coordinator = parts.coordinator()
    let links = coordinator.settingsLinks()
    #expect(links.menuBarIcon() == .orbit)
    links.chooseMenuBarIcon(.screen)
    #expect(parts.menuBarIcon == .screen)
    #expect(links.menuBarIcon() == .screen)
  }
}
