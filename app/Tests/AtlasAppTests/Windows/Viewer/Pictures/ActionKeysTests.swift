import Testing

@testable import AtlasApp

@Suite("ActionKeys")
struct ActionKeysTests {
  @Test("over an action shown large, ← and → step, B and A pick the picture, and Esc closes")
  func largeKeys() {
    let event = { (key: UInt16) in ActionKeys.event(keyCode: key, hasModifiers: false, large: true, replaying: false) }
    #expect(event(ShotKeys.left) == .previousAction)
    #expect(event(ShotKeys.right) == .nextAction)
    #expect(event(ActionKeys.before) == .compareAction(.before))
    #expect(event(ActionKeys.after) == .compareAction(.after))
    #expect(event(ShotKeys.escape) == .closeAction)
    #expect(event(ShotKeys.space) == nil)
  }

  @Test("over a replay, ← and → step, Space plays or pauses, and Esc closes")
  func replayKeys() {
    let event = { (key: UInt16) in ActionKeys.event(keyCode: key, hasModifiers: false, large: false, replaying: true) }
    #expect(event(ShotKeys.left) == .previousFrame)
    #expect(event(ShotKeys.right) == .nextFrame)
    #expect(event(ShotKeys.space) == .togglePlay)
    #expect(event(ShotKeys.escape) == .closeReplay)
    #expect(event(ActionKeys.before) == nil)
  }

  @Test("keys held with a modifier, or with nothing shown over Activity, mean nothing here")
  func otherwiseNothing() {
    #expect(ActionKeys.event(keyCode: ShotKeys.left, hasModifiers: true, large: true, replaying: false) == nil)
    #expect(ActionKeys.event(keyCode: ShotKeys.left, hasModifiers: false, large: false, replaying: false) == nil)
  }
}
