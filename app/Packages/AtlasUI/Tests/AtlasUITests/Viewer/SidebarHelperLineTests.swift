import Testing

@testable import AtlasUI

@Suite("SidebarHelperLine")
@MainActor
struct SidebarHelperLineTests {
  @Test("a click on the helper line opens Settings on Helper, as the menu's own item does")
  func aClickOpensSettingsOnHelper() {
    #expect(SidebarHelperLine.click == .menu(.openSettings))
  }

  @Test("the right-click menu lists the helper's facts, then Check Again and Open Settings on Helper…, in every state")
  func theRightClickMenuKeepsItsSections() throws {
    for sample in PreviewStates.viewerHelperLines {
      let sections = SidebarHelperLine.contextMenu(sample.value)
      #expect(sections == sample.value.menu, "\(sample.name)")
      let commands = sections.flatMap(\.items)
        .compactMap(\.command)
      #expect(commands.contains(.openSettings), "\(sample.name) offers Open Settings on Helper…")
      #expect(commands.contains(.checkAgain), "\(sample.name) offers Check Again")
      #expect(sections.first?.items.allSatisfy(\.isInfo) == true, "\(sample.name) opens on the helper's facts")
    }
  }
}
