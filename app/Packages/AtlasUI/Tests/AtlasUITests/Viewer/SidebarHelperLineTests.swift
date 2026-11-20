import CoreGraphics
import Testing

@testable import AtlasUI

@Suite("SidebarHelperLine")
@MainActor
struct SidebarHelperLineTests {
  @Test("a click on the helper line opens Settings on Helper, as the menu's own item does")
  func aClickOpensSettingsOnHelper() {
    #expect(SidebarHelperLine.click == .menu(.openSettings))
  }

  @Test("with one desktop a click still opens Settings, and with two it opens the desktops over the row")
  func aClickOpensTheDesktopsOnceThereAreTwo() {
    let frame = CGRect(x: 8, y: 840, width: 220, height: 44)
    let one = PreviewStates.viewerHelperReady(logsOut: true)
    #expect(!one.picksDesktop)
    #expect(SidebarHelperLine.clicked(one, frame: frame) == .menu(.openSettings))
    let two = PreviewStates.viewerTwoDesktops.sidebar.helper
    #expect(two.picksDesktop)
    #expect(SidebarHelperLine.clicked(two, frame: frame) == .showDesktops(from: frame))
    #expect(two.desktops.filter(\.isViewed).map(\.title) == ["work"], "the desktop shown is the one checked")
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
