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
    #expect(ShotKeys.event(keyCode: ShotKeys.escape, hasModifiers: false, largeShown: false) == nil)
    #expect(ShotKeys.event(keyCode: ShotKeys.right, hasModifiers: true, largeShown: true) == nil)
  }
}
