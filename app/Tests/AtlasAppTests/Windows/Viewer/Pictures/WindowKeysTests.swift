import AtlasUI
import Testing

@testable import AtlasApp

@Suite("WindowKeys")
struct WindowKeysTests {
  @Test("Escape on the Screen stops showing the outlined window, but not while this window holds control")
  func escapeClearsTheOutline() {
    #expect(
      WindowKeys.screenEvent(keyCode: WindowKeys.escape, hasModifiers: false, holdingControl: false, outlineShown: true)
        == .clearOutline
    )
    #expect(
      WindowKeys.screenEvent(keyCode: WindowKeys.escape, hasModifiers: false, holdingControl: true, outlineShown: true)
        == nil,
      "in control, Escape is the helper's"
    )
    #expect(
      WindowKeys.screenEvent(
        keyCode: WindowKeys.escape,
        hasModifiers: false,
        holdingControl: false,
        outlineShown: false
      )
        == nil
    )
    #expect(
      WindowKeys.screenEvent(keyCode: WindowKeys.space, hasModifiers: false, holdingControl: false, outlineShown: true)
        == nil
    )
  }

  @Test("Escape puts away the sidebar shown over the card, but not while this window holds control")
  func escapePutsAwayTheSidebarOverTheCard() {
    #expect(
      WindowKeys.overlayEvent(keyCode: WindowKeys.escape, hasModifiers: false, holdingControl: false)
        == .dismissSidebarOverlay
    )
    #expect(WindowKeys.overlayEvent(keyCode: WindowKeys.escape, hasModifiers: false, holdingControl: true) == nil)
    #expect(WindowKeys.overlayEvent(keyCode: WindowKeys.escape, hasModifiers: true, holdingControl: false) == nil)
    #expect(WindowKeys.overlayEvent(keyCode: WindowKeys.space, hasModifiers: false, holdingControl: false) == nil)
  }
}
