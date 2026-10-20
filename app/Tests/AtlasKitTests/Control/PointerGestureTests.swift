import Foundation
import Testing

@testable import AtlasKit

@Suite("PointerGesture")
struct PointerGestureTests {
  let point = DesktopPoint(x: 100, y: 200)

  func phase(_ action: InputAction?) -> InputAction.Phase? {
    if case .pointer(let phase, _, _, _, _, _) = action { phase } else { nil }
  }

  func count(_ action: InputAction?) -> Int? {
    if case .pointer(_, _, _, _, let count, _) = action { count } else { nil }
  }

  @Test("clicks in one place soon after count up to a triple click")
  func clickCount() {
    var gesture = PointerGesture()
    #expect(phase(gesture.start(.left, at: point, modifiers: [], now: 0)) == .down)
    #expect(gesture.move(.left, to: point, modifiers: []) == nil)
    #expect(phase(gesture.finish(.left, at: point, modifiers: [], now: 0.02)) == .up)
    #expect(count(gesture.start(.left, at: point, modifiers: [], now: 0.05)) == 2)
    _ = gesture.finish(.left, at: point, modifiers: [], now: 0.07)
    #expect(count(gesture.start(.left, at: point, modifiers: [], now: 0.1)) == 3)
    _ = gesture.finish(.left, at: point, modifiers: [], now: 0.12)
    #expect(count(gesture.start(.left, at: point, modifiers: [], now: 0.15)) == 3, "never past a triple click")
    _ = gesture.finish(.left, at: point, modifiers: [], now: 0.17)
    #expect(count(gesture.start(.left, at: point, modifiers: [], now: 1)) == 1, "too late to count")
  }

  @Test("a drag moves before its release and ends the click count")
  func drag() {
    var gesture = PointerGesture()
    _ = gesture.start(.left, at: point, modifiers: [], now: 0)
    let moved = DesktopPoint(x: 120, y: 210)
    #expect(phase(gesture.move(.left, to: moved, modifiers: [])) == .move)
    #expect(gesture.isActive)
    #expect(gesture.move(.left, to: moved, modifiers: []) == nil, "no move without movement")
    _ = gesture.finish(.left, at: moved, modifiers: [], now: 0.05)
    #expect(count(gesture.start(.left, at: moved, modifiers: [], now: 0.1)) == 1, "a drag is not a click")
  }

  @Test("another button or place starts a new click")
  func newClick() {
    var gesture = PointerGesture()
    _ = gesture.start(.left, at: point, modifiers: [], now: 0)
    _ = gesture.finish(.left, at: point, modifiers: [], now: 0.01)
    #expect(count(gesture.start(.right, at: point, modifiers: [], now: 0.05)) == 1)
    _ = gesture.finish(.right, at: point, modifiers: [], now: 0.06)
    #expect(count(gesture.start(.right, at: DesktopPoint(x: 150, y: 200), modifiers: [], now: 0.1)) == 1)
  }

  @Test("another button coming up does not end the drag")
  func otherButton() {
    var gesture = PointerGesture()
    _ = gesture.start(.left, at: point, modifiers: [], now: 0)
    #expect(gesture.start(.right, at: point, modifiers: [], now: 0.1) == nil)
    #expect(gesture.finish(.right, at: point, modifiers: [], now: 0.2) == nil)
    #expect(gesture.move(.right, to: DesktopPoint(x: 140, y: 200), modifiers: []) == nil)
    #expect(phase(gesture.move(.left, to: DesktopPoint(x: 140, y: 200), modifiers: [])) == .move)
    #expect(phase(gesture.finish(.left, at: DesktopPoint(x: 140, y: 200), modifiers: [], now: 0.3)) == .up)
  }

  @Test("a cancelled gesture sends nothing")
  func cancelled() {
    var gesture = PointerGesture()
    _ = gesture.start(.left, at: point, modifiers: [], now: 0)
    gesture.cancel()
    #expect(gesture.finish(.left, at: point, modifiers: [], now: 0.1) == nil)
    #expect(gesture.start(.left, at: point, modifiers: [], now: 0.2) != nil)
    #expect(gesture.start(.left, at: point, modifiers: [], now: 0.3) == nil, "one button at a time")
  }

  @Test("only the press carries the click count")
  func pressCount() throws {
    var gesture = PointerGesture()
    _ = gesture.start(.left, at: point, modifiers: [], now: 0)
    _ = gesture.finish(.left, at: point, modifiers: [], now: 0.01)
    _ = gesture.start(.left, at: point, modifiers: [.shift], now: 0.05)
    let up = gesture.finish(.left, at: point, modifiers: [.shift], now: 0.06)
    #expect(count(up) == 1)
    let encoder = JSONEncoder()
    let json = try #require(
      try JSONSerialization.jsonObject(with: encoder.encode(up)) as? [String: Any]
    )
    #expect(json["modifiers"] as? [String] == ["shift"])
  }
}
