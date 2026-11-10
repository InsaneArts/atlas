import AppKit
import AsyncSupportTesting
import AtlasCommandsTesting
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("SettingsWindow", .serialized, AppKitState())
struct SettingsWindowTests {
  @Test("the Settings window stands at its fixed 740 by 572, keeping its top-left corner where it fits")
  func theWindowKeepsItsSizeAndCorner() {
    let current = CGRect(x: 100, y: 300, width: 680, height: 500)
    let frame = SettingsWindowFrame.frame(current: current)
    #expect(frame.size == CGSize(width: SettingsLayout.width, height: SettingsLayout.height))
    #expect(frame.minX == current.minX)
    #expect(frame.maxY == current.maxY, "the top edge stays")
    let screen = CGRect(x: 0, y: 40, width: 1440, height: 860)
    #expect(SettingsWindowFrame.frame(current: frame, within: screen) == frame, "a window that fits stays put")
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

  @Test("the window is not resizable, and its title follows the route shown")
  @MainActor
  func theTitleFollowsTheRoute() async {
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]))
    let window = SettingsWindow(controller: controller) { _ in }
    #expect(!window.styleMask.contains(.resizable))
    #expect(window.title == "Helper")
    controller.send(.select(.privacy))
    await afterQueuedWork()
    #expect(window.title == "Privacy")
  }

  @Test("the mouse's back and forward buttons move through the window's history")
  @MainActor
  func theMouseButtonsGoBackAndForward() throws {
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]))
    let window = SettingsWindow(controller: controller) { _ in }
    controller.send(.select(.about))
    window.otherMouseDown(with: try Self.press(button: SettingsWindow.backButton, in: window))
    #expect(controller.state.route == .page(.helper))
    window.otherMouseDown(with: try Self.press(button: SettingsWindow.forwardButton, in: window))
    #expect(controller.state.route == .page(.about))
  }

  /// A press of a mouse's other button in `window`.
  ///
  /// - Parameters:
  ///   - button: The button's number.
  ///   - window: The window.
  /// - Returns: The press.
  /// - Throws: When AppKit makes no event.
  @MainActor
  private static func press(button: Int, in window: NSWindow) throws -> NSEvent {
    let event = try #require(
      CGEvent(mouseEventSource: nil, mouseType: .otherMouseDown, mouseCursorPosition: .zero, mouseButton: .center)
    )
    event.setIntegerValueField(.mouseEventButtonNumber, value: Int64(button))
    return try #require(NSEvent(cgEvent: event))
  }
}
