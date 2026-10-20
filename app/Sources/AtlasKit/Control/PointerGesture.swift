/// A mouse button held and moved: down, moves while held, up. Repeated
/// clicks in the same place soon after count up to a triple click.
package struct PointerGesture: Sendable {
  /// A click this soon after the last, in seconds, counts as a repeat.
  static let repeatWithin = 0.5
  /// A click this close to the last, in points, counts as a repeat.
  static let repeatNear = 10.0
  /// Moving further than this, in points, makes a press a drag.
  static let dragFrom = 3.0

  /// The button held now.
  struct Active {
    let button: MouseButton
    let count: Int
    let from: DesktopPoint
    var last: DesktopPoint
    var moved = false
  }

  /// The last click, which a repeat counts from.
  struct Previous {
    let button: MouseButton
    let count: Int
    let at: Double
    let point: DesktopPoint
  }

  var active: Active?
  var previous: Previous?

  /// Creates a gesture with no button held.
  package init() {}

  /// Whether a button is held.
  package var isActive: Bool { active != nil }
  /// Where the held button last was.
  package var last: DesktopPoint? { active?.last }

  /// A button goes down at `point` at time `now` (seconds).
  ///
  /// - Parameters:
  ///   - button: The button pressed.
  ///   - point: Where, on the helper's display.
  ///   - modifiers: The modifier keys held.
  ///   - now: The time, in seconds.
  /// - Returns: The press with its click count, or `nil` while another button is held.
  package mutating func start(
    _ button: MouseButton,
    at point: DesktopPoint,
    modifiers: Modifiers,
    now: Double
  )
    -> InputAction?
  {
    guard active == nil else { return nil }
    var count = 1
    if let previous, previous.button == button, now - previous.at < Self.repeatWithin,
      previous.point.distance(to: point) < Self.repeatNear
    {
      count = min(3, previous.count + 1)
    }
    active = Active(button: button, count: count, from: point, last: point)
    return .pointer(phase: .down, x: point.x, y: point.y, button: button, count: count, modifiers: modifiers.names)
  }

  /// The held `button` moved to `point`. Another button is not this gesture's.
  ///
  /// - Parameters:
  ///   - button: The button that moved.
  ///   - point: Where it is now, on the helper's display.
  ///   - modifiers: The modifier keys held.
  /// - Returns: The move, or `nil` for another button or no movement.
  package mutating func move(_ button: MouseButton, to point: DesktopPoint, modifiers: Modifiers) -> InputAction? {
    guard var current = active, current.button == button, point != current.last else { return nil }
    current.last = point
    current.moved = current.moved || point.distance(to: current.from) > Self.dragFrom
    active = current
    // Only the press carries the click count, as the service expects.
    return .pointer(
      phase: .move,
      x: point.x,
      y: point.y,
      button: current.button,
      count: 1,
      modifiers: modifiers.names
    )
  }

  /// The held `button` comes up at `point`. Another button coming up ends nothing.
  ///
  /// - Parameters:
  ///   - button: The button released.
  ///   - point: Where, on the helper's display.
  ///   - modifiers: The modifier keys held.
  ///   - now: The time, in seconds.
  /// - Returns: The release, or `nil` for a button this gesture does not hold.
  package mutating func finish(
    _ button: MouseButton,
    at point: DesktopPoint,
    modifiers: Modifiers,
    now: Double
  )
    -> InputAction?
  {
    guard let current = active, current.button == button else { return nil }
    previous =
      current.moved ? nil : Previous(button: current.button, count: current.count, at: now, point: point)
    active = nil
    return .pointer(
      phase: .up,
      x: point.x,
      y: point.y,
      button: current.button,
      count: 1,
      modifiers: modifiers.names
    )
  }

  /// Forgets the held button and the last click, so nothing more is sent for them.
  package mutating func cancel() {
    active = nil
    previous = nil
  }
}
