import AppKit
import AsyncSupportTesting
import AtlasCommandsTesting
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("SettingsWindow", .serialized, AppKitState())
struct SettingsWindowTests {
  @Test("the Settings window is never smaller than 740 by 572, keeping its top-left corner where it fits")
  func theWindowKeepsItsSmallestSizeAndCorner() {
    let current = CGRect(x: 100, y: 300, width: 680, height: 500)
    let frame = SettingsWindowFrame.frame(current: current)
    #expect(frame.size == CGSize(width: SettingsLayout.width, height: SettingsLayout.height))
    #expect(frame.minX == current.minX)
    #expect(frame.maxY == current.maxY, "the top edge stays")
    let screen = CGRect(x: 0, y: 40, width: 1440, height: 860)
    #expect(SettingsWindowFrame.frame(current: frame, within: screen) == frame, "a window that fits stays put")
  }

  @Test("a size the person left stays, and shrinks to a smaller screen without going under the smallest")
  func aLeftSizeStays() {
    let screen = CGRect(x: 0, y: 40, width: 1440, height: 860)
    let larger = CGRect(x: 100, y: 100, width: 1000, height: 700)
    #expect(SettingsWindowFrame.frame(current: larger, within: screen) == larger)
    let small = CGRect(x: 0, y: 0, width: 900, height: 600)
    let fitted = SettingsWindowFrame.frame(current: larger, within: small)
    #expect(fitted.size == CGSize(width: 900, height: 600))
    let tiny = CGRect(x: 0, y: 0, width: 600, height: 400)
    #expect(SettingsWindowFrame.frame(current: larger, within: tiny).size == SettingsWindowFrame.smallest)
  }

  @Test("a place restored past the screen's edges moves just enough to stand whole on it")
  func aRestoredPlaceIsKeptOnScreen() {
    let screen = CGRect(x: 0, y: 40, width: 1440, height: 860)
    let pastRight = CGRect(x: 1200, y: 300, width: SettingsLayout.width, height: SettingsLayout.height)
    #expect(SettingsWindowFrame.frame(current: pastRight, within: screen).maxX == screen.maxX)
    let pastBottom = CGRect(x: 100, y: -200, width: SettingsLayout.width, height: SettingsLayout.height)
    #expect(SettingsWindowFrame.frame(current: pastBottom, within: screen).minY == screen.minY)
    let pastTop = CGRect(x: 100, y: 600, width: SettingsLayout.width, height: SettingsLayout.height)
    #expect(SettingsWindowFrame.frame(current: pastTop, within: screen).maxY == screen.maxY)
    let small = CGRect(x: 0, y: 0, width: 600, height: 400)
    let held = SettingsWindowFrame.frame(current: pastRight, within: small)
    #expect(held.minX == small.minX && held.maxY == small.maxY, "a screen too small holds it by its top-left corner")
  }

  @Test("the window resizes from its smallest size up, and its title follows the route shown")
  @MainActor
  func theTitleFollowsTheRoute() async {
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]))
    let window = SettingsWindow(controller: controller) { _ in }
    #expect(window.styleMask.contains(.resizable))
    #expect(window.contentMinSize == SettingsWindowFrame.smallest)
    #expect(window.title == "Helper")
    controller.send(.select(.privacy))
    await afterQueuedWork()
    #expect(window.title == "Privacy")
  }
}
