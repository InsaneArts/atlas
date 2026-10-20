/// One input action for the helper, exactly as the service's `input`
/// endpoint takes it (`src/lib/actions.rs`). Coordinates are display points.
package enum InputAction: Encodable, Equatable, Sendable {
  /// Where a pointer action is in a press.
  package enum Phase: String, Encodable, Sendable {
    /// The button goes down.
    case down
    /// The button moves while held.
    case move
    /// The button comes up.
    case up
  }

  /// A button going down, moving while held, or coming up.
  case pointer(phase: Phase, x: Double, y: Double, button: MouseButton, count: Int, modifiers: [String])
  /// The pointer moves with no button held.
  case move(x: Double, y: Double)
  /// Scrolls by whole lines at a point.
  case scroll(x: Double, y: Double, dx: Int, dy: Int)
  /// Types committed text.
  case type(text: String)
  /// Presses a key or shortcut, such as `cmd+v`.
  case key(String)
  /// Waits, in milliseconds.
  case wait(milliseconds: Int)

  /// Every field an action may carry; each case writes only its own.
  enum CodingKeys: String, CodingKey {
    case type, phase, x, y, button, count, modifiers, dx, dy, text, key, ms
  }

  /// Encodes the action with its `type` (``kind``) and only its own fields.
  ///
  /// - Parameter encoder: The encoder writing the `input` request's body.
  /// - Throws: `EncodingError` when a value cannot be encoded.
  package func encode(to encoder: any Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    try container.encode(kind, forKey: .type)
    switch self {
    case .pointer(let phase, let x, let y, let button, let count, let modifiers):
      try container.encode(phase, forKey: .phase)
      try container.encode(x, forKey: .x)
      try container.encode(y, forKey: .y)
      try container.encode(button, forKey: .button)
      try container.encode(count, forKey: .count)
      try container.encode(modifiers, forKey: .modifiers)
    case .move(let x, let y):
      try container.encode(x, forKey: .x)
      try container.encode(y, forKey: .y)
    case .scroll(let x, let y, let dx, let dy):
      try container.encode(x, forKey: .x)
      try container.encode(y, forKey: .y)
      try container.encode(dx, forKey: .dx)
      try container.encode(dy, forKey: .dy)
    case .type(let text):
      try container.encode(text, forKey: .text)
    case .key(let key):
      try container.encode(key, forKey: .key)
    case .wait(let milliseconds):
      try container.encode(milliseconds, forKey: .ms)
    }
  }
}
