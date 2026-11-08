import Testing

@testable import AtlasApp

@Suite("ActionKeys")
struct ActionKeysTests {
  @Test("over the picture sheet, ← and → step, B and A pick the picture, and Esc closes")
  func sheetKeys() {
    let event = { (key: UInt16) in ActionKeys.event(keyCode: key, hasModifiers: false, sheet: true, replaying: false) }
    #expect(event(WindowKeys.left) == .previousPicture)
    #expect(event(WindowKeys.right) == .nextPicture)
    #expect(event(ActionKeys.before) == .pictureSide(.before))
    #expect(event(ActionKeys.after) == .pictureSide(.after))
    #expect(event(WindowKeys.escape) == .closePicture)
    #expect(event(WindowKeys.space) == nil)
  }

  @Test("over a replay, ← and → step, Space plays or pauses, and Esc closes")
  func replayKeys() {
    let event = { (key: UInt16) in ActionKeys.event(keyCode: key, hasModifiers: false, sheet: false, replaying: true) }
    #expect(event(WindowKeys.left) == .previousFrame)
    #expect(event(WindowKeys.right) == .nextFrame)
    #expect(event(WindowKeys.space) == .togglePlay)
    #expect(event(WindowKeys.escape) == .closeReplay)
    #expect(event(ActionKeys.before) == nil)
  }

  @Test("keys held with a modifier, or with nothing shown over Activity, mean nothing here")
  func otherwiseNothing() {
    #expect(ActionKeys.event(keyCode: WindowKeys.left, hasModifiers: true, sheet: true, replaying: false) == nil)
    #expect(ActionKeys.event(keyCode: WindowKeys.left, hasModifiers: false, sheet: false, replaying: false) == nil)
  }
}
