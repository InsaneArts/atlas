import AppKit
import AtlasKit

/// Control: the lease this window holds, its heartbeat, and the input that
/// belongs to it. The service checks every action against the lease and
/// the picture it was aimed at; this side only stops sending early.
extension ViewerModel {
    /// Take control with the picture on screen. A reply that arrives after
    /// the person changed their mind, or left the window, is given back.
    public func takeControl() async {
        guard canTakeControl, let frame else { return }
        acquiring = true
        controlGeneration += 1
        let generation = controlGeneration
        do {
            let granted = try await service.acquire(frame: frame.id)
            guard generation == controlGeneration, focused, shown, !paused, ready else {
                Task { try? await service.release(lease: granted) }
                if generation == controlGeneration { acquiring = false }
                return
            }
            lease = granted
            acquiring = false
            input.activate(granted)
            say(nil)
            // Renew every 2.5 s; end control once the picture is 6.5 s old,
            // whether or not the person is still giving input.
            heartbeat = Task { [weak self] in
                var tick = 0
                while !Task.isCancelled {
                    try? await Task.sleep(for: .milliseconds(500))
                    guard !Task.isCancelled, let self, lease == granted else { return }
                    guard !pictureStale else {
                        releaseControl("The picture stopped updating. Wait for a fresh one, then choose Control.")
                        return
                    }
                    tick += 1
                    if tick % 5 == 0 { await renew(granted) }
                }
            }
            refresh()
        } catch {
            guard generation == controlGeneration else { return }
            acquiring = false
            say(error.localizedDescription)
        }
    }

    private func renew(_ owner: String) async {
        guard lease == owner, let frame else { return }
        do {
            try await service.renew(lease: owner, frame: frame.id)
        } catch {
            if lease == owner { releaseControl(error.localizedDescription) }
        }
    }

    /// Give control back: at once here, then to the service. Also cancels an
    /// acquisition still under way, whose late grant is then given back.
    public func releaseControl(_ message: String? = nil) {
        controlGeneration += 1
        let generation = controlGeneration
        let previous = lease
        lease = nil
        acquiring = false
        heartbeat?.cancel()
        heartbeat = nil
        clearInput()
        if let previous {
            Task {
                do {
                    try await service.release(lease: previous)
                    // Show the release at once, not at the next reading.
                    if lease == nil && generation == controlGeneration { await readStatus() }
                } catch {
                    if lease == nil && generation == controlGeneration { say(error.localizedDescription) }
                }
            }
        }
        if let message { say(message) }
    }

    /// The window stopped taking the keyboard: control goes back.
    public func lostFocus() {
        focused = false
        releaseControl()
    }

    public func gainedFocus() {
        focused = true
    }

    func clearInput() {
        input.activate(nil)
        wheel.reset()
        gesture.cancel()
        wheelAt = nil
        wheelSending?.cancel()
        wheelSending = nil
    }

    /// Whether input may go to the helper now; a picture gone stale ends control.
    /// The last picture is too old to aim input at.
    var pictureStale: Bool {
        guard let frameAt else { return true }
        return clock() - frameAt > 6.5
    }

    var canInput: Bool {
        guard lease != nil, frame != nil, !paused, focused, shown else { return false }
        if pictureStale {
            releaseControl("The picture stopped updating. Wait for a fresh one, then choose Control.")
            return false
        }
        return true
    }

    func enqueue(_ actions: [InputAction]) {
        guard canInput, let frame else { return }
        input.enqueue(actions, frame: frame.id)
    }

    // MARK: Input from the desktop view

    /// The display point under `location` in a view of `size`.
    func point(_ location: CGPoint, in size: CGSize, clamp: Bool = false) -> DesktopPoint? {
        guard let frame, let image else { return nil }
        return desktopPoint(location, in: size, image: image.size, display: frame.display, clamp: clamp)
    }

    func pointerDown(_ button: MouseButton, at location: CGPoint, in size: CGSize, modifiers: Modifiers) {
        guard canInput, let point = point(location, in: size),
            let action = gesture.start(button, at: point, modifiers: modifiers, now: Self.now)
        else { return }
        enqueue([action])
    }

    func pointerDragged(_ button: MouseButton, to location: CGPoint, in size: CGSize, modifiers: Modifiers) {
        guard canInput, let point = point(location, in: size, clamp: true),
            let action = gesture.move(button, to: point, modifiers: modifiers)
        else { return }
        enqueue([action])
    }

    /// The button comes up where it is, or where the gesture last was.
    func pointerUp(_ button: MouseButton, at location: CGPoint, in size: CGSize, modifiers: Modifiers) {
        guard let point = point(location, in: size, clamp: true) ?? gesture.last else { return }
        let movement = gesture.move(button, to: point, modifiers: modifiers)
        guard let action = gesture.finish(button, at: point, modifiers: modifiers, now: Self.now), canInput else {
            return
        }
        enqueue([movement, action].compactMap { $0 })
    }

    func pointerMoved(to location: CGPoint, in size: CGSize) {
        guard canInput, !gesture.isActive, let point = point(location, in: size) else { return }
        enqueue([.move(x: point.x, y: point.y)])
    }

    /// Scrolling is gathered for a moment and sent as whole lines.
    func scrolled(dx: Double, dy: Double, precise: Bool, at location: CGPoint, in size: CGSize) {
        guard canInput, let point = point(location, in: size) else { return }
        wheel.add(dx: dx, dy: dy, precise: precise)
        wheelAt = point
        guard wheelSending == nil else { return }
        wheelSending = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(70))
            guard let self, !Task.isCancelled else { return }
            wheelSending = nil
            let delta = wheel.take()
            guard let at = wheelAt, delta.dx != 0 || delta.dy != 0 else { return }
            enqueue([
                .move(x: at.x, y: at.y), .wait(milliseconds: 80),
                .scroll(x: at.x, y: at.y, dx: delta.dx, dy: delta.dy),
            ])
        }
    }

    /// A key press: release, paste, a key, or nil when it is text for the
    /// input method. Returns whether it was handled.
    func keyPressed(keyCode: UInt16, modifiers: Modifiers, clipboard: () -> String?) -> Bool {
        guard canInput else { return false }
        if KeyMap.isRelease(keyCode: keyCode, modifiers: modifiers) {
            releaseControl()
            return true
        }
        if KeyMap.isPaste(keyCode: keyCode, modifiers: modifiers) {
            send(text: clipboard() ?? "")
            return true
        }
        guard let action = KeyMap.action(keyCode: keyCode, modifiers: modifiers) else { return false }
        enqueue([action])
        return true
    }

    /// A key from the dock (Escape, a shortcut).
    public func sendKey(_ key: String) {
        enqueue([.key(key)])
    }

    /// Committed text, typed in order, in chunks the helper accepts.
    func send(text: String) {
        guard !text.isEmpty, canInput else { return }
        guard text.unicodeScalars.count <= 2048 else {
            say("Paste is limited to 2,048 characters at a time.")
            return
        }
        let chunks = textChunks(text)
        guard chunks.allSatisfy({ $0.unicodeScalars.count <= 256 }) else {
            say("This text contains a character sequence too long to type.")
            return
        }
        for chunk in chunks { enqueue([.type(text: chunk)]) }
    }

    static var now: Double { ProcessInfo.processInfo.systemUptime }
}

extension ViewerModel: InputSink {
    public func send(lease: String, frame: String, actions: [InputAction]) async throws {
        try await service.input(lease: lease, frame: frame, actions: actions)
    }

    public func inputSent() { refresh() }

    public func inputFailed(_ error: any Error) { releaseControl(error.localizedDescription) }
}
