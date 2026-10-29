import AtlasUI
import Testing

@testable import AtlasApp

@Suite("ShotKeys")
struct ShotKeysTests {
  @Test("Space, the arrows and Esc move through the screenshots, and keys held with ⌘ pass by")
  func keys() {
    #expect(ShotKeys.event(keyCode: ShotKeys.space, hasModifiers: false, largeShown: false) == .toggleShot)
    #expect(ShotKeys.event(keyCode: ShotKeys.left, hasModifiers: false, largeShown: true) == .previousShot)
    #expect(ShotKeys.event(keyCode: ShotKeys.right, hasModifiers: false, largeShown: false) == .nextShot)
    #expect(ShotKeys.event(keyCode: ShotKeys.escape, hasModifiers: false, largeShown: true) == .closeShot)
    #expect(ShotKeys.event(keyCode: ShotKeys.right, hasModifiers: true, largeShown: true) == nil)
  }

  @Test("in the grid the up and down arrows move a row, Return opens the selected one and Esc clears the selection")
  func gridKeys() {
    #expect(ShotKeys.event(keyCode: ShotKeys.up, hasModifiers: false, largeShown: false) == .shotAbove)
    #expect(ShotKeys.event(keyCode: ShotKeys.down, hasModifiers: false, largeShown: false) == .shotBelow)
    #expect(ShotKeys.event(keyCode: ShotKeys.returnKey, hasModifiers: false, largeShown: false) == .toggleShot)
    #expect(ShotKeys.event(keyCode: ShotKeys.escape, hasModifiers: false, largeShown: false) == .clearShotSelection)
    // With one shown large, the rows and Return mean nothing, and ⌘C goes on to the Edit menu.
    #expect(ShotKeys.event(keyCode: ShotKeys.up, hasModifiers: false, largeShown: true) == nil)
    #expect(ShotKeys.event(keyCode: ShotKeys.returnKey, hasModifiers: false, largeShown: true) == nil)
  }

  @Test("Escape on the Screen stops showing the outlined window, but not while this window holds control")
  func escapeClearsTheOutline() {
    #expect(
      ShotKeys.screenEvent(keyCode: ShotKeys.escape, hasModifiers: false, holdingControl: false, outlineShown: true)
        == .clearOutline
    )
    #expect(
      ShotKeys.screenEvent(keyCode: ShotKeys.escape, hasModifiers: false, holdingControl: true, outlineShown: true)
        == nil,
      "in control, Escape is the helper's"
    )
    #expect(
      ShotKeys.screenEvent(keyCode: ShotKeys.escape, hasModifiers: false, holdingControl: false, outlineShown: false)
        == nil
    )
    #expect(
      ShotKeys.screenEvent(keyCode: ShotKeys.space, hasModifiers: false, holdingControl: false, outlineShown: true)
        == nil
    )
  }

  @Test("Escape puts away the sidebar shown over the card, but not while this window holds control")
  func escapePutsAwayTheSidebarOverTheCard() {
    #expect(
      ShotKeys.overlayEvent(keyCode: ShotKeys.escape, hasModifiers: false, holdingControl: false)
        == .dismissSidebarOverlay
    )
    #expect(ShotKeys.overlayEvent(keyCode: ShotKeys.escape, hasModifiers: false, holdingControl: true) == nil)
    #expect(ShotKeys.overlayEvent(keyCode: ShotKeys.escape, hasModifiers: true, holdingControl: false) == nil)
    #expect(ShotKeys.overlayEvent(keyCode: ShotKeys.space, hasModifiers: false, holdingControl: false) == nil)
  }
}
