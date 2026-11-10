import Testing

@testable import AtlasUI

@Suite("SettingsSidebarState")
@MainActor
struct SettingsSidebarStateTests {
  /// The sample sidebar, searched for `query`.
  ///
  /// - Parameter query: What the person typed.
  /// - Returns: The pages the search leaves.
  private static func pages(_ query: String) -> [SettingsPage] {
    var sidebar = PreviewStates.settingsSidebar
    sidebar.query = query
    return sidebar.visibleRows.map(\.page)
  }

  @Test("an empty search shows every page, in order")
  func anEmptySearchShowsEveryPage() {
    #expect(Self.pages("") == SettingsPage.allCases)
    #expect(Self.pages("  ") == SettingsPage.allCases)
  }

  @Test("a search finds a page by its title or by the words of its rows, ignoring case")
  func aSearchFindsTitlesAndWords() {
    #expect(Self.pages("priv") == [.privacy])
    #expect(Self.pages("MUTE") == [.general])
    #expect(Self.pages("claude") == [.integrations])
    #expect(Self.pages("typed") == [.activity])
  }

  @Test("a search that matches nothing leaves no rows, which the sidebar says")
  func aSearchWithNoMatchLeavesNone() {
    #expect(Self.pages("zebra").isEmpty)
  }

  @Test("the sidebar groups Helper, then General and Activity, then the services, then About")
  func thePagesGroup() {
    #expect(SettingsPage.allCases.map(\.group) == [0, 1, 1, 2, 2, 2, 3])
  }
}
