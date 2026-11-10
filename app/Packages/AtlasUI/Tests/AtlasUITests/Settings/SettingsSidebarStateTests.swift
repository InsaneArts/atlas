import Testing

@testable import AtlasUI

@Suite("SettingsSidebarState")
@MainActor
struct SettingsSidebarStateTests {
  @Test("the sidebar lists every page in order, each titled by its row")
  func theSidebarListsEveryPage() {
    let sidebar = PreviewStates.settingsSidebar
    #expect(sidebar.rows.map(\.page) == SettingsPage.allCases)
    #expect(sidebar.title(of: .integrations) == "Integrations")
  }

  @Test("the sidebar groups Helper, then General and Activity, then the services, then About")
  func thePagesGroup() {
    #expect(SettingsPage.allCases.map(\.group) == [0, 1, 1, 2, 2, 2, 3])
  }

  @Test("each page but Helper, which shows the helper's mark, has a symbol of its own")
  func eachPageHasItsSymbol() {
    let symbols = SettingsPage.allCases
      .filter { $0 != .helper }
      .map(\.symbol)
    #expect(Set(symbols).count == symbols.count)
  }

  @Test("Settings' rows answer presses and selection but never light under the pointer, where the viewer's still do")
  func settingsRowsDoNotHover() {
    #expect(!PillRowStyle(isSelected: false, hovers: false).hovers)
    #expect(PillRowStyle().hovers)
  }
}
