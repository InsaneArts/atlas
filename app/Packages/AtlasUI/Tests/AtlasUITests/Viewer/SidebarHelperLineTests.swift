import Testing

@testable import AtlasUI

@Suite("SidebarHelperLine")
struct SidebarHelperLineTests {
  @Test("a click on the helper line opens Settings on Helper, as the menu's own item does")
  func aClickOpensSettingsOnHelper() {
    #expect(SidebarHelperLine.click == .menu(.openSettings))
  }

  @MainActor
  @Test("the helper's menu stays for the right-click in every state")
  func theRightClickMenuKeepsEverySection() {
    for sample in PreviewStates.viewerHelperLines {
      #expect(!sample.value.menu.isEmpty, "\(sample.name) offers its menu")
    }
  }
}
