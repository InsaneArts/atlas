import Foundation
import Testing

@testable import AtlasKit

@Suite struct MappingTests {
    @Test func mappingUsesTheContainedImageAndDisplayPointsIncludingRetina() {
        let view = CGSize(width: 400, height: 400)
        let image = CGSize(width: 1600, height: 900)
        let display = ViewerStatus.Size(width: 1280, height: 720)
        // The picture is letterboxed: 400 × 225, from y 87.5.
        #expect(desktopPoint(CGPoint(x: 80, y: 10), in: view, image: image, display: display) == nil)
        #expect(
            desktopPoint(CGPoint(x: 200, y: 200), in: view, image: image, display: display)
                == DesktopPoint(x: 640, y: 360))
        #expect(desktopPoint(CGPoint(x: 480, y: 170), in: view, image: image, display: display) == nil)
    }

    @Test(arguments: [CGPoint(x: -10, y: 70), CGPoint(x: 410, y: 70), CGPoint(x: 80, y: -20), CGPoint(x: 80, y: 320)])
    func activeDragsClampAtEachEdge(location: CGPoint) {
        let view = CGSize(width: 400, height: 300)
        let image = CGSize(width: 800, height: 600)
        let display = ViewerStatus.Size(width: 1600, height: 1200)
        #expect(desktopPoint(location, in: view, image: image, display: display) == nil)
        let point = desktopPoint(location, in: view, image: image, display: display, clamp: true)
        #expect(point.map { $0.x >= 0 && $0.x < 1600 && $0.y >= 0 && $0.y < 1200 } == true)
    }

    @Test func emptySizesMapNowhere() {
        #expect(
            desktopPoint(
                CGPoint(x: 0, y: 0), in: CGSize(width: 0, height: 0), image: CGSize(width: 1, height: 1),
                display: .init(width: 1, height: 1))
                == nil)
    }
}

@Suite struct KeyTests {
    @Test func textIsLeftToTheInputMethodNamedKeysAndShortcutsAreKeys() {
        // "e" alone is text; Command-A and Shift-Left are keys.
        #expect(KeyMap.action(keyCode: 14, modifiers: []) == nil)
        #expect(KeyMap.action(keyCode: 14, modifiers: .option) == nil)
        #expect(KeyMap.action(keyCode: 0, modifiers: .command) == .key("cmd+a"))
        #expect(KeyMap.action(keyCode: 123, modifiers: .shift) == .key("shift+left"))
        #expect(KeyMap.action(keyCode: 36, modifiers: []) == .key("return"))
        #expect(KeyMap.action(keyCode: 122, modifiers: []) == .key("f1"))
        #expect(
            KeyMap.action(keyCode: 0, modifiers: [.command, .shift, .control, .option]) == .key("cmd+ctrl+alt+shift+a"))
    }

    @Test(arguments: [(UInt16(33), "["), (44, "/"), (24, "="), (49, "space")])
    func punctuationShortcutsMapPhysicalKeys(keyCode: UInt16, key: String) {
        #expect(KeyMap.action(keyCode: keyCode, modifiers: .command) == .key("cmd+\(key)"))
    }

    @Test func onlyCommandVIsTheLocalPasteGesture() {
        #expect(KeyMap.isPaste(keyCode: 9, modifiers: .command))
        #expect(!KeyMap.isPaste(keyCode: 9, modifiers: .control))
        #expect(!KeyMap.isPaste(keyCode: 9, modifiers: [.command, .shift]))
        #expect(!KeyMap.isPaste(keyCode: 9, modifiers: [.command, .option]))
        #expect(KeyMap.action(keyCode: 9, modifiers: .control) == .key("ctrl+v"))
    }

    @Test func controlShiftEscapeGivesControlBack() {
        #expect(KeyMap.isRelease(keyCode: 53, modifiers: [.control, .shift]))
        #expect(!KeyMap.isRelease(keyCode: 53, modifiers: .control))
        #expect(KeyMap.action(keyCode: 53, modifiers: []) == .key("escape"))
    }
}

@Suite struct WheelTests {
    @Test func bigDeltasAreCappedAtAHundredLines() {
        var wheel = WheelAccumulator()
        wheel.add(dx: -80000, dy: 80000, precise: true)
        #expect(wheel.take() == (-100, 100))
    }

    @Test func smallTrackpadDeltasSurviveSendsAndReversals() {
        var wheel = WheelAccumulator()
        var total = 0
        for _ in 0..<30 {
            wheel.add(dx: 0, dy: -2, precise: true)
            total += wheel.take().dy
        }
        #expect(total == -6)
        for _ in 0..<30 {
            wheel.add(dx: 0, dy: 2, precise: true)
            total += wheel.take().dy
        }
        #expect(total == 0)
    }

    @Test func wheelLinesAreSentAsLines() {
        var wheel = WheelAccumulator()
        wheel.add(dx: 0, dy: 3, precise: false)
        #expect(wheel.take() == (0, 3))
    }
}

@Suite struct GestureTests {
    let point = DesktopPoint(x: 100, y: 200)

    func phase(_ action: InputAction?) -> InputAction.Phase? {
        if case .pointer(let phase, _, _, _, _, _) = action { phase } else { nil }
    }

    func count(_ action: InputAction?) -> Int? {
        if case .pointer(_, _, _, _, let count, _) = action { count } else { nil }
    }

    @Test func stationaryClicksHaveNoDragAndCountUpWhenRepeated() {
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

    @Test func aDragMovesBeforeReleaseAndEndsTheCount() {
        var gesture = PointerGesture()
        _ = gesture.start(.left, at: point, modifiers: [], now: 0)
        let moved = DesktopPoint(x: 120, y: 210)
        #expect(phase(gesture.move(.left, to: moved, modifiers: [])) == .move)
        #expect(gesture.isActive)
        #expect(gesture.move(.left, to: moved, modifiers: []) == nil, "no move without movement")
        _ = gesture.finish(.left, at: moved, modifiers: [], now: 0.05)
        #expect(count(gesture.start(.left, at: moved, modifiers: [], now: 0.1)) == 1, "a drag is not a click")
    }

    @Test func anotherButtonOrPlaceIsANewClick() {
        var gesture = PointerGesture()
        _ = gesture.start(.left, at: point, modifiers: [], now: 0)
        _ = gesture.finish(.left, at: point, modifiers: [], now: 0.01)
        #expect(count(gesture.start(.right, at: point, modifiers: [], now: 0.05)) == 1)
        _ = gesture.finish(.right, at: point, modifiers: [], now: 0.06)
        #expect(count(gesture.start(.right, at: DesktopPoint(x: 150, y: 200), modifiers: [], now: 0.1)) == 1)
    }

    @Test func anotherButtonComingUpDoesNotEndTheDrag() {
        var gesture = PointerGesture()
        _ = gesture.start(.left, at: point, modifiers: [], now: 0)
        #expect(gesture.start(.right, at: point, modifiers: [], now: 0.1) == nil)
        #expect(gesture.finish(.right, at: point, modifiers: [], now: 0.2) == nil)
        #expect(gesture.move(.right, to: DesktopPoint(x: 140, y: 200), modifiers: []) == nil)
        #expect(phase(gesture.move(.left, to: DesktopPoint(x: 140, y: 200), modifiers: [])) == .move)
        #expect(phase(gesture.finish(.left, at: DesktopPoint(x: 140, y: 200), modifiers: [], now: 0.3)) == .up)
    }

    @Test func aCancelledGestureSendsNothing() {
        var gesture = PointerGesture()
        _ = gesture.start(.left, at: point, modifiers: [], now: 0)
        gesture.cancel()
        #expect(gesture.finish(.left, at: point, modifiers: [], now: 0.1) == nil)
        #expect(gesture.start(.left, at: point, modifiers: [], now: 0.2) != nil)
        #expect(gesture.start(.left, at: point, modifiers: [], now: 0.3) == nil, "one button at a time")
    }

    @Test func onlyThePressCarriesTheClickCount() throws {
        var gesture = PointerGesture()
        _ = gesture.start(.left, at: point, modifiers: [], now: 0)
        _ = gesture.finish(.left, at: point, modifiers: [], now: 0.01)
        _ = gesture.start(.left, at: point, modifiers: [.shift], now: 0.05)
        let up = gesture.finish(.left, at: point, modifiers: [.shift], now: 0.06)
        #expect(count(up) == 1)
        let json = try #require(
            try JSONSerialization.jsonObject(with: JSONEncoder().encode(up)) as? [String: Any])
        #expect(json["modifiers"] as? [String] == ["shift"])
    }
}

@Suite struct TextTests {
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"
    let accent = "e\u{301}"

    @Test func typedTextIsChunkedWithoutSplittingGraphemes() {
        #expect(textChunks("abcdef", size: 4) == ["abcd", "ef"])
        #expect(textChunks("ab\(family)", size: 4) == ["ab", family])
        #expect(textChunks("abc\(accent)d", size: 4) == ["abc", "\(accent)d"])
        let long = String(repeating: "\(accent)\(family)", count: 40)
        let chunks = textChunks(long)
        #expect(chunks.allSatisfy { $0.unicodeScalars.count <= 128 })
        #expect(chunks.joined() == long)
        #expect(textChunks(family, size: 2) == [family], "an oversized grapheme stays whole")
        #expect(textChunks("").isEmpty)
    }
}

@MainActor
final class Sink: InputSink {
    var sent: [String] = []
    var failures: [String] = []
    var hold: CheckedContinuation<Void, any Error>?
    var holdLease: String?

    func send(lease: String, frame: String, actions: [InputAction]) async throws {
        sent.append(lease)
        if lease == holdLease {
            try await withCheckedThrowingContinuation { hold = $0 }
        }
    }
    func inputSent() {}
    func inputFailed(_ error: any Error) { failures.append(error.localizedDescription) }
}

@MainActor
@Suite struct QueueTests {
    @Test func anOldRequestsFailureCannotStrandANewLease() async {
        let sink = Sink()
        sink.holdLease = "old"
        let queue = InputQueue()
        queue.sink = sink
        queue.activate("old")
        queue.enqueue([.key("return")], frame: "frame")
        await settle { sink.hold != nil }
        queue.activate(nil)
        queue.activate("new")
        queue.enqueue([.key("escape")], frame: "frame")
        sink.hold?.resume(throwing: ServiceError("old request disconnected"))
        await settle { sink.sent.count == 2 }
        #expect(sink.sent == ["old", "new"])
        #expect(sink.failures.isEmpty)
        #expect(queue.pending.isEmpty)
    }

    @Test func aNewerMoveReplacesAQueuedOne() async {
        let sink = Sink()
        sink.holdLease = "lease"
        let queue = InputQueue()
        queue.sink = sink
        queue.activate("lease")
        queue.enqueue([.key("a")], frame: "f")
        await settle { sink.hold != nil }
        queue.enqueue([.move(x: 1, y: 1)], frame: "f")
        queue.enqueue([.move(x: 2, y: 2)], frame: "f")
        #expect(queue.pending.count == 1)
        #expect(queue.pending.first?.actions == [.move(x: 2, y: 2)])
        sink.hold?.resume()
    }

    @Test func tooMuchWaitingEndsControl() async {
        let sink = Sink()
        sink.holdLease = "lease"
        let queue = InputQueue()
        queue.sink = sink
        queue.activate("lease")
        queue.enqueue([.key("a")], frame: "f")
        await settle { sink.hold != nil }
        for index in 0...InputQueue.limit { queue.enqueue([.key("k\(index)")], frame: "f") }
        #expect(sink.failures.count == 1)
        #expect(queue.pending.count == InputQueue.limit)
        sink.hold?.resume()
    }

    @Test func nothingIsQueuedWithoutALease() {
        let queue = InputQueue()
        queue.enqueue([.key("a")], frame: "f")
        #expect(queue.pending.isEmpty)
    }
}
