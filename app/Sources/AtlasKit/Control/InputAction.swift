import Foundation

/// A mouse button, as the service names it.
public enum MouseButton: String, Codable, Sendable {
    case left, middle, right
}

/// One input action for the helper, exactly as the service's `input`
/// endpoint takes it (`src/lib/actions.rs`). Coordinates are display points.
public enum InputAction: Encodable, Equatable, Sendable {
    public enum Phase: String, Encodable, Sendable { case down, move, up }

    /// A button going down, moving while held, or coming up.
    case pointer(phase: Phase, x: Double, y: Double, button: MouseButton, count: Int, modifiers: [String])
    case move(x: Double, y: Double)
    case scroll(x: Double, y: Double, dx: Int, dy: Int)
    case type(text: String)
    case key(String)
    case wait(milliseconds: Int)

    enum CodingKeys: String, CodingKey {
        case type, phase, x, y, button, count, modifiers, dx, dy, text, key, ms
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .pointer(let phase, let x, let y, let button, let count, let modifiers):
            try container.encode("pointer", forKey: .type)
            try container.encode(phase, forKey: .phase)
            try container.encode(x, forKey: .x)
            try container.encode(y, forKey: .y)
            try container.encode(button, forKey: .button)
            try container.encode(count, forKey: .count)
            try container.encode(modifiers, forKey: .modifiers)
        case .move(let x, let y):
            try container.encode("move", forKey: .type)
            try container.encode(x, forKey: .x)
            try container.encode(y, forKey: .y)
        case .scroll(let x, let y, let dx, let dy):
            try container.encode("scroll", forKey: .type)
            try container.encode(x, forKey: .x)
            try container.encode(y, forKey: .y)
            try container.encode(dx, forKey: .dx)
            try container.encode(dy, forKey: .dy)
        case .type(let text):
            try container.encode("type", forKey: .type)
            try container.encode(text, forKey: .text)
        case .key(let key):
            try container.encode("key", forKey: .type)
            try container.encode(key, forKey: .key)
        case .wait(let milliseconds):
            try container.encode("wait", forKey: .type)
            try container.encode(milliseconds, forKey: .ms)
        }
    }

    /// A pointer or plain move, which a newer one may replace while queued.
    public var isMove: Bool {
        switch self {
        case .move: true
        case .pointer(let phase, _, _, _, _, _): phase == .move
        default: false
        }
    }
}
