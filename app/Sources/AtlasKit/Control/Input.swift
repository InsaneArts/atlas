import Foundation

/// A point on the helper's display, in its points.
public struct DesktopPoint: Equatable, Sendable {
    public var x: Double
    public var y: Double

    public init(x: Double, y: Double) {
        self.x = x
        self.y = y
    }

    func distance(to other: DesktopPoint) -> Double { hypot(x - other.x, y - other.y) }
}

/// Where a point in the view showing the desktop falls on the helper's
/// display. The view shows the picture letterboxed at its own shape; a
/// point outside the picture is nowhere, unless `clamp`ed to its edge (a
/// drag that left the view). `location` is from the view's top left.
public func desktopPoint(
    _ location: CGPoint, in view: CGSize, image: CGSize, display: ViewerStatus.Size, clamp: Bool = false
) -> DesktopPoint? {
    let sizes = [view.width, view.height, image.width, image.height, display.width, display.height]
    guard sizes.allSatisfy({ $0.isFinite && $0 > 0 }) else { return nil }
    let scale = min(view.width / image.width, view.height / image.height)
    let width = image.width * scale
    let height = image.height * scale
    var x = location.x - (view.width - width) / 2
    var y = location.y - (view.height - height) / 2
    if clamp {
        x = max(0, min(width - 0.001, x))
        y = max(0, min(height - 0.001, y))
    } else if x < 0 || y < 0 || x >= width || y >= height {
        return nil
    }
    return DesktopPoint(x: x / width * display.width, y: y / height * display.height)
}

/// The modifier keys held, as the service names them, in its order.
public struct Modifiers: OptionSet, Sendable, Hashable {
    public let rawValue: Int
    public init(rawValue: Int) { self.rawValue = rawValue }

    public static let command = Modifiers(rawValue: 1)
    public static let control = Modifiers(rawValue: 2)
    public static let option = Modifiers(rawValue: 4)
    public static let shift = Modifiers(rawValue: 8)

    public var names: [String] {
        [(Modifiers.command, "cmd"), (.control, "ctrl"), (.option, "alt"), (.shift, "shift")]
            .filter { contains($0.0) }
            .map(\.1)
    }
}

/// Keys as the service names them, from Mac virtual key codes.
public enum KeyMap {
    /// Keys that are always sent as keys, never as text.
    static let named: [UInt16: String] = [
        36: "return", 76: "return", 48: "tab", 51: "backspace", 117: "forwarddelete", 53: "escape",
        123: "left", 124: "right", 125: "down", 126: "up",
        115: "home", 119: "end", 116: "pageup", 121: "pagedown",
        122: "f1", 120: "f2", 99: "f3", 118: "f4", 96: "f5", 97: "f6",
        98: "f7", 100: "f8", 101: "f9", 109: "f10", 103: "f11", 111: "f12",
    ]

    /// The key at each position on the keyboard, as on a US layout: a
    /// shortcut names the key pressed, whatever the layout prints on it.
    static let positions: [UInt16: String] = [
        0: "a", 11: "b", 8: "c", 2: "d", 14: "e", 3: "f", 5: "g", 4: "h", 34: "i", 38: "j", 40: "k",
        37: "l", 46: "m", 45: "n", 31: "o", 35: "p", 12: "q", 15: "r", 1: "s", 17: "t", 32: "u", 9: "v",
        13: "w", 7: "x", 16: "y", 6: "z",
        29: "0", 18: "1", 19: "2", 20: "3", 21: "4", 23: "5", 22: "6", 26: "7", 28: "8", 25: "9",
        33: "[", 30: "]", 42: "\\", 44: "/", 27: "-", 24: "=", 41: ";", 39: "'", 50: "`", 43: ",",
        47: ".", 49: "space",
    ]

    /// The key action for a key press, or nil when it is text (which
    /// arrives committed, through the input method) or nothing at all.
    public static func action(keyCode: UInt16, modifiers: Modifiers) -> InputAction? {
        var key = named[keyCode]
        if key == nil && (modifiers.contains(.command) || modifiers.contains(.control)) {
            key = positions[keyCode]
        }
        guard let key else { return nil }
        return .key((modifiers.names + [key]).joined(separator: "+"))
    }

    /// Only Command-V is the paste gesture: it sends this Mac's clipboard
    /// as typed text. Control-V and the shortcut menu send a key.
    public static func isPaste(keyCode: UInt16, modifiers: Modifiers) -> Bool {
        modifiers == .command && keyCode == 9
    }

    /// Control-Shift-Escape gives control back.
    public static func isRelease(keyCode: UInt16, modifiers: Modifiers) -> Bool {
        keyCode == 53 && modifiers.contains(.control) && modifiers.contains(.shift)
    }
}

/// Committed text as type actions of at most `size` code points each,
/// never splitting a grapheme (an emoji sequence, a letter and its marks).
/// A single grapheme longer than `size` is its own chunk; callers refuse
/// chunks the helper would not accept.
public func textChunks(_ text: String, size: Int = 128) -> [String] {
    var chunks: [String] = []
    var current = ""
    var length = 0
    for grapheme in text {
        let points = grapheme.unicodeScalars.count
        if !current.isEmpty && length + points > size {
            chunks.append(current)
            current = ""
            length = 0
        }
        current.append(grapheme)
        length += points
    }
    if !current.isEmpty { chunks.append(current) }
    return chunks
}

/// Scrolling kept in fractions across sends: rounding each event would drop
/// slow trackpad movement before the helper ever saw it.
public struct WheelAccumulator: Sendable {
    var x = 0.0
    var y = 0.0

    public init() {}

    public mutating func reset() {
        x = 0
        y = 0
    }

    /// Add one scroll event's deltas: points for a trackpad (`precise`),
    /// lines for a wheel. Positive moves content down and right, as AppKit
    /// has it; the sender turns that into Atlas's lines down the page. A
    /// line is a wheel notch, about ten points in the apps measured.
    public mutating func add(dx: Double, dy: Double, precise: Bool) {
        let factor = precise ? 1.0 / 10 : 1
        x += dx * factor
        y += dy * factor
    }

    /// The whole lines to send now (at most 100 each way); the rest stays.
    public mutating func take() -> (dx: Int, dy: Int) {
        func whole(_ value: Double) -> Int {
            Int(max(-100, min(100, (value + (value < 0 ? -1e-9 : value > 0 ? 1e-9 : 0)).rounded(.towardZero))))
        }
        let dx = whole(x)
        let dy = whole(y)
        x -= Double(dx)
        y -= Double(dy)
        return (dx, dy)
    }
}

/// A mouse button held and moved: down, moves while held, up. Repeated
/// clicks in the same place soon after count up to a triple click.
public struct PointerGesture: Sendable {
    /// A click this soon after the last, in seconds, counts as a repeat.
    static let repeatWithin = 0.5
    /// A click this close to the last, in points, counts as a repeat.
    static let repeatNear = 10.0
    /// Moving further than this, in points, makes a press a drag.
    static let dragFrom = 3.0

    struct Active {
        let button: MouseButton
        let count: Int
        let from: DesktopPoint
        var last: DesktopPoint
        var moved = false
    }
    struct Previous {
        let button: MouseButton
        let count: Int
        let at: Double
        let point: DesktopPoint
    }

    var active: Active?
    var previous: Previous?

    public init() {}

    public var isActive: Bool { active != nil }
    /// Where the held button last was.
    public var last: DesktopPoint? { active?.last }

    /// A button goes down at `point` at time `now` (seconds).
    public mutating func start(
        _ button: MouseButton, at point: DesktopPoint, modifiers: Modifiers, now: Double
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
    public mutating func move(_ button: MouseButton, to point: DesktopPoint, modifiers: Modifiers) -> InputAction? {
        guard var current = active, current.button == button, point != current.last else { return nil }
        current.last = point
        current.moved = current.moved || point.distance(to: current.from) > Self.dragFrom
        active = current
        // Only the press carries the click count, as the service expects.
        return .pointer(
            phase: .move, x: point.x, y: point.y, button: current.button, count: 1, modifiers: modifiers.names)
    }

    /// The held `button` comes up at `point`. Another button coming up ends nothing.
    public mutating func finish(
        _ button: MouseButton, at point: DesktopPoint, modifiers: Modifiers, now: Double
    )
        -> InputAction?
    {
        guard let current = active, current.button == button else { return nil }
        previous =
            current.moved ? nil : Previous(button: current.button, count: current.count, at: now, point: point)
        active = nil
        return .pointer(
            phase: .up, x: point.x, y: point.y, button: current.button, count: 1, modifiers: modifiers.names)
    }

    public mutating func cancel() {
        active = nil
        previous = nil
    }
}

/// Where the queue's input goes, and who hears how it went.
@MainActor
public protocol InputSink: AnyObject {
    func send(lease: String, frame: String, actions: [InputAction]) async throws
    /// A batch was sent.
    func inputSent()
    /// A batch failed; control should end.
    func inputFailed(_ error: any Error)
}

/// Input owned by the lease that made it, even across slow replies: sent in
/// order, one batch at a time; a newer move replaces a queued one; too much
/// waiting at once ends control instead of falling behind.
@MainActor
public final class InputQueue {
    public static let limit = 32

    struct Item {
        let actions: [InputAction]
        let frame: String
        let lease: String
        let generation: Int
    }

    public weak var sink: (any InputSink)?
    private(set) var owner: String?
    private var generation = 0
    private(set) var pending: [Item] = []
    private var sending = false

    public init() {}

    /// Input from now on belongs to `owner`; anything queued is dropped.
    public func activate(_ owner: String?) {
        generation += 1
        self.owner = owner
        pending = []
    }

    public func enqueue(_ actions: [InputAction], frame: String) {
        guard let owner else { return }
        if actions.count == 1, let last = pending.last, last.actions.count == 1, actions[0].isMove,
            last.actions[0].isMove, actions[0].kind == last.actions[0].kind
        {
            pending.removeLast()
        }
        guard pending.count < Self.limit else {
            sink?.inputFailed(
                ServiceError(
                    "Input is arriving faster than the helper can handle it. Take control again when ready."))
            return
        }
        pending.append(Item(actions: actions, frame: frame, lease: owner, generation: generation))
        Task { await drain() }
    }

    func drain() async {
        guard !sending else { return }
        sending = true
        while owner != nil, !pending.isEmpty, let sink {
            let item = pending.removeFirst()
            guard item.generation == generation else { continue }
            do {
                try await sink.send(lease: item.lease, frame: item.frame, actions: item.actions)
                if item.generation == generation { sink.inputSent() }
            } catch {
                if item.generation == generation { sink.inputFailed(error) }
            }
        }
        sending = false
    }
}

extension InputAction {
    /// The action's type, as the service names it.
    var kind: String {
        switch self {
        case .pointer: "pointer"
        case .move: "move"
        case .scroll: "scroll"
        case .type: "type"
        case .key: "key"
        case .wait: "wait"
        }
    }
}
