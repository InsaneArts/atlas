import Foundation
import Testing

@testable import AtlasKit

@Suite("PointerGesture")
struct PointerGestureTests {
  let point = DesktopPoint(x: 100, y: 200)
  /// The Mac's double-click speed out of the box, in seconds.
  let within = 0.5

  func phase(_ action: InputAction?) -> InputAction.Phase? {
    if case .pointer(let phase, _, _, _, _, _) = action { phase } else { nil }
  }

  func count(_ action: InputAction?) -> Int? {
    if case .pointer(_, _, _, _, let count, _) = action { count } else { nil }
  }

  /// A press on a view drawn at `scale`, with the double-click speed `within` unless given another.
  func press(
    _ gesture: inout PointerGesture,
    _ button: MouseButton = .left,
    at point: DesktopPoint,
    scale: Double = 1,
    modifiers: Modifiers = [],
    now: Double,
    within: Double? = nil
  ) -> InputAction? {
    gesture.start(
      button,
      at: point,
      scale: scale,
      modifiers: modifiers,
      now: now,
      repeatWithin: within ?? self.within
    )
  }

  @Test("clicks in one place soon after count up to a triple click")
  func clickCount() {
    var gesture = PointerGesture()
    #expect(phase(press(&gesture, at: point, now: 0)) == .down)
    #expect(gesture.move(.left, to: point, scale: 1, modifiers: []) == nil)
    #expect(phase(gesture.finish(.left, at: point, modifiers: [], now: 0.02)) == .up)
    #expect(count(press(&gesture, at: point, now: 0.05)) == 2)
    _ = gesture.finish(.left, at: point, modifiers: [], now: 0.07)
    #expect(count(press(&gesture, at: point, now: 0.1)) == 3)
    _ = gesture.finish(.left, at: point, modifiers: [], now: 0.12)
    #expect(count(press(&gesture, at: point, now: 0.15)) == 3, "never past a triple click")
    _ = gesture.finish(.left, at: point, modifiers: [], now: 0.17)
    #expect(count(press(&gesture, at: point, now: 1)) == 1, "too late to count")
  }

  @Test("a repeat counts within the Mac's double-click speed, not a fixed one")
  func repeatFollowsTheDoubleClickSpeed() {
    var slow = PointerGesture()
    _ = press(&slow, at: point, now: 0, within: 1.2)
    _ = slow.finish(.left, at: point, modifiers: [], now: 0.05)
    #expect(count(press(&slow, at: point, now: 0.8, within: 1.2)) == 2)
    var fast = PointerGesture()
    _ = press(&fast, at: point, now: 0, within: 0.2)
    _ = fast.finish(.left, at: point, modifiers: [], now: 0.05)
    #expect(count(press(&fast, at: point, now: 0.3, within: 0.2)) == 1)
  }

  @Test("a drag moves before its release and ends the click count")
  func drag() {
    var gesture = PointerGesture()
    _ = press(&gesture, at: point, now: 0)
    let moved = DesktopPoint(x: 120, y: 210)
    #expect(phase(gesture.move(.left, to: moved, scale: 1, modifiers: [])) == .move)
    #expect(gesture.isActive)
    #expect(gesture.move(.left, to: moved, scale: 1, modifiers: []) == nil, "no move without movement")
    _ = gesture.finish(.left, at: moved, modifiers: [], now: 0.05)
    #expect(count(press(&gesture, at: moved, now: 0.1)) == 1, "a drag is not a click")
  }

  @Test("repeat and drag distances are the view's, so a viewer drawn at half size still double-clicks")
  func distancesAreMeasuredOnTheView() {
    // 16 display points apart and a 5-point wobble: past both thresholds at full size, within them at half size.
    let near = DesktopPoint(x: 116, y: 200)
    let wobble = DesktopPoint(x: 105, y: 200)
    var half = PointerGesture()
    _ = press(&half, at: point, scale: 0.5, now: 0)
    _ = half.move(.left, to: wobble, scale: 0.5, modifiers: [])
    _ = half.finish(.left, at: wobble, modifiers: [], now: 0.05)
    #expect(count(press(&half, at: near, scale: 0.5, now: 0.1)) == 2)
    var full = PointerGesture()
    _ = press(&full, at: point, now: 0)
    _ = full.finish(.left, at: point, modifiers: [], now: 0.05)
    #expect(count(press(&full, at: near, now: 0.1)) == 1)
  }

  @Test("another button or place starts a new click")
  func newClick() {
    var gesture = PointerGesture()
    _ = press(&gesture, at: point, now: 0)
    _ = gesture.finish(.left, at: point, modifiers: [], now: 0.01)
    #expect(count(press(&gesture, .right, at: point, now: 0.05)) == 1)
    _ = gesture.finish(.right, at: point, modifiers: [], now: 0.06)
    #expect(count(press(&gesture, .right, at: DesktopPoint(x: 150, y: 200), now: 0.1)) == 1)
  }

  @Test("another button coming up does not end the drag")
  func otherButton() {
    var gesture = PointerGesture()
    let moved = DesktopPoint(x: 140, y: 200)
    _ = press(&gesture, at: point, now: 0)
    #expect(press(&gesture, .right, at: point, now: 0.1) == nil)
    #expect(gesture.finish(.right, at: point, modifiers: [], now: 0.2) == nil)
    #expect(gesture.move(.right, to: moved, scale: 1, modifiers: []) == nil)
    #expect(phase(gesture.move(.left, to: moved, scale: 1, modifiers: [])) == .move)
    #expect(phase(gesture.finish(.left, at: moved, modifiers: [], now: 0.3)) == .up)
  }

  @Test("a cancelled gesture sends nothing")
  func cancelled() {
    var gesture = PointerGesture()
    _ = press(&gesture, at: point, now: 0)
    gesture.cancel()
    #expect(gesture.finish(.left, at: point, modifiers: [], now: 0.1) == nil)
    #expect(press(&gesture, at: point, now: 0.2) != nil)
    #expect(press(&gesture, at: point, now: 0.3) == nil, "one button at a time")
  }

  @Test("only the press carries the click count")
  func pressCount() throws {
    var gesture = PointerGesture()
    _ = press(&gesture, at: point, now: 0)
    _ = gesture.finish(.left, at: point, modifiers: [], now: 0.01)
    _ = press(&gesture, at: point, modifiers: [.shift], now: 0.05)
    let up = gesture.finish(.left, at: point, modifiers: [.shift], now: 0.06)
    #expect(count(up) == 1)
    let encoder = JSONEncoder()
    let json = try #require(
      try JSONSerialization.jsonObject(with: encoder.encode(up)) as? [String: Any]
    )
    #expect(json["modifiers"] as? [String] == ["shift"])
  }
}
