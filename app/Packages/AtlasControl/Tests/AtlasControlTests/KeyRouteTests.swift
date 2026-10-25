import Testing

@testable import AtlasControl

@Suite("KeyRoute")
struct KeyRouteTests {
  @Test("Control-Shift-Escape gives control back at any moment, mid-composition included", arguments: [false, true])
  func releaseWorksAlways(composing: Bool) {
    #expect(KeyRoute(keyCode: 53, modifiers: [.control, .shift], typesQ: false, composing: composing) == .release)
  }

  @Test("Command-Q stays on this Mac, by the key that types Q in any layout")
  func quitStaysOnThisMac() {
    #expect(KeyRoute(keyCode: 0, modifiers: .command, typesQ: true, composing: false) == .passOn)
    #expect(
      KeyRoute(keyCode: 12, modifiers: .command, typesQ: false, composing: false) == .send(endingComposition: false)
    )
  }

  @Test("mid-composition a key belongs to the input method, and a shortcut ends the composition first")
  func compositionKeepsItsKeys() {
    #expect(KeyRoute(keyCode: 14, modifiers: [], typesQ: false, composing: true) == .passOn)
    #expect(KeyRoute(keyCode: 0, modifiers: .command, typesQ: false, composing: true) == .send(endingComposition: true))
  }

  @Test("a key the helper did not take goes on as text, but a shortcut is kept from this app")
  func unhandledShortcutsAreSwallowed() {
    #expect(KeyRoute.passesOn(handled: false, modifiers: []))
    #expect(!KeyRoute.passesOn(handled: false, modifiers: .command))
    #expect(!KeyRoute.passesOn(handled: true, modifiers: []))
  }

  @Test(
    "a held Control-Shift-Escape acts once: its repeats are swallowed, so it never takes back the control it gave",
    arguments: [
      (53, [.control, .shift], true, true),
      (53, [.control, .shift], false, false),
      (53, [], true, false),
    ] as [(UInt16, Modifiers, Bool, Bool)]
  )
  func heldReleaseActsOnce(keyCode: UInt16, modifiers: Modifiers, repeating: Bool, swallowed: Bool) {
    #expect(KeyRoute.swallows(keyCode: keyCode, modifiers: modifiers, repeating: repeating) == swallowed)
  }
}
