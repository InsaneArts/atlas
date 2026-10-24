import Testing

@testable import AtlasUI

@Suite("SettingsSidebarTile")
struct SettingsSidebarTileTests {
  @Test("the Agents category tile uses the terminal symbol")
  @MainActor
  func agentsUsesTheTerminalSymbol() {
    #expect(SettingsSidebarTile(pane: .agents).symbol == .terminal)
  }
}
