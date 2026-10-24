import AppKit
import Testing

@testable import AtlasApp

@Suite("MoreMenu")
struct MoreMenuTests {
  /// Answers the menu's items in a test.
  final class Target: NSObject {}

  @Test("⋯ holds only what acts on the picture: Copy, Save, Actual Size and how often pictures arrive")
  func moreHoldsOnlyThePicturesCommands() {
    let value = ViewerMoreMenu(hasPicture: true, actsOnShot: false, actualSize: true, rate: .everyFiveSeconds)
    let menu = MoreMenu.menu(value, target: Target())
    let titles = menu.items.map { $0.isSeparatorItem ? "—" : $0.title }
    #expect(titles == ["Copy Picture", "Save Picture…", "Actual Size", "—", "Updates", "Live", "Every 5 Seconds"])
    #expect(menu.items.filter { $0.state == .on }.map(\.title) == ["Actual Size", "Every 5 Seconds"])
    #expect(menu.items.allSatisfy { $0.keyEquivalent != "p" }, "pausing is the pause button's and ⇧⌘P's")
  }

  @Test("on the Screenshots tab ⋯ copies and saves the screenshot")
  func moreActsOnTheScreenshot() {
    let value = ViewerMoreMenu(hasPicture: true, actsOnShot: true, actualSize: false, rate: .live)
    let titles = MoreMenu.menu(value, target: Target()).items
      .prefix(2)
      .map(\.title)
    #expect(titles == ["Copy Screenshot", "Save Screenshot…"])
  }

  @Test("the top row's menus open under their buttons, lined up with their trailing edge")
  func menusLineUpWithTheirButtonsTrailingEdge() {
    let button = CGRect(x: 1100, y: 11, width: 30, height: 30)
    let origin = ViewerWindow.menuOrigin(below: button, width: 250)
    #expect(origin == NSPoint(x: 1130 - 250, y: 41 + ViewerWindow.menuGap))
  }
}
