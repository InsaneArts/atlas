import Testing

@testable import AtlasUI

@Suite("SettingsRoute desktops")
@MainActor
struct SettingsRouteDesktopsTests {
  @Test("a desktop's page sits under Desktops and goes back there once it is not listed, or one desktop is left")
  func keptDesktops() {
    let route = SettingsRoute.desktop("vm")
    #expect(route.page == .helper)
    #expect(route.desktop == "vm")
    #expect(route.account == nil)
    #expect(route.kept(desktops: ["user", "vm"]) == route)
    #expect(route.kept(desktops: ["user", "other"]) == .page(.helper))
    #expect(route.kept(desktops: ["vm"]) == .page(.helper), "the one desktop shows in Helper's place")
    #expect(SettingsRoute.account("a").kept(desktops: []) == .account("a"))
  }

  @Test("a desktop's page is titled by its row, and Desktops by the sidebar")
  func titles() {
    var state = PreviewStates.settingsVM
    #expect(state.title == "work")
    state.route = .page(.helper)
    #expect(state.title == "Desktops")
  }
}
