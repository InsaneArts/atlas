import AtlasControl
import AtlasService
import AtlasStatus
import Foundation
import Observation

/// Control: the lease this window holds, its heartbeat, its release, and the input that belongs to it: pointer,
/// scrolling, keys and text, each sent only while input may go. The service checks every action against the lease and
/// the picture it was aimed at; this side only stops sending early, and waits for every release to be answered before
/// control can be taken again. ``ViewerController`` owns one; its state is written here alone.
@Observable
final class ViewerControl {
  /// One heartbeat tick: 500 ms, so a stale picture ends control within half a second; the lease is renewed every
  /// ``renewEvery`` ticks.
  static let heartbeatEvery: Duration = .milliseconds(500)
  /// The lease is renewed every this many heartbeat ticks: 5, every 2.5 s at ``heartbeatEvery``.
  static let renewEvery = 5
  /// How long Take Control waits before asking again after the service answers busy: 1 s. The service is busy while
  /// one of an agent's actions runs, and a click or a short burst of keys ends well within a second.
  static let busyRetryAfter: Duration = .seconds(1)
  /// How many times Take Control asks again while the service answers busy: 4, a second apart, so the press is
  /// accepted and control comes as soon as the agent's step ends, within about five seconds; a step longer than that
  /// is not worth holding the button for, so the person is told to try again instead.
  static let busyRetries = 4
  /// How long Take Control pressed while the picture is paused waits for the first fresh picture after resuming it:
  /// 5 s, several readings' worth even at the slower rate.
  static let freshPictureWait: Duration = .seconds(5)
  /// Scroll events are gathered this long, then sent as whole lines: 70 ms, short enough to feel immediate and long
  /// enough to gather a trackpad's burst into one batch.
  static let scrollGathering: Duration = .milliseconds(70)
  /// The pause between the move to where scrolling happens and the scroll: 80 ms, so the app under the pointer sees
  /// the pointer arrive before the wheel turns.
  static let scrollSettle = 80
  /// A wheel's pause this long ends its gesture: 500 ms, far longer than the gap between a slow wheel's notches, so
  /// the fraction a slow turn gathers is kept. A trackpad says itself when a gesture begins.
  static let newScrollAfter: Duration = .milliseconds(500)
  /// The least time between the readings input asks for: 200 ms. Each batch sent asks to read again, and a reading
  /// is a fresh capture and a PNG, so a moving mouse would otherwise keep them running back to back; five a second
  /// still shows what the input did.
  static let inputReadEvery: Duration = .milliseconds(200)

  /// The lease this window holds while in control.
  private(set) var lease: String?
  /// Whether this window's acquisition is under way, its one retry included.
  private(set) var acquiring = false
  /// Text the input method is composing, not yet sent.
  private(set) var composing = ""

  /// The window this control belongs to: its picture, what it is doing, and its notice. Weak, as the window owns it.
  @ObservationIgnored private weak var viewer: ViewerController?
  /// The viewer service it asks.
  @ObservationIgnored private let service: any ViewerService
  /// The helper's own commands, shared with the panel.
  @ObservationIgnored private let actions: HelperActions
  /// The app's control work not yet answered, shared with the helper's commands and any later window.
  private var barrier: ControlBarrier { actions.barrier }
  /// The time now, in seconds of uptime, which never steps back: what a picture's age is told by.
  @ObservationIgnored private let uptime: @MainActor () -> TimeInterval
  /// The clock every wait runs on; a cancelled wait ends at once.
  @ObservationIgnored private let clock: any Clock<Duration>
  /// One heartbeat tick; the lease is renewed every ``renewEvery`` ticks.
  @ObservationIgnored private let heartbeatInterval: Duration
  /// The Mac's double-click speed, in seconds.
  @ObservationIgnored private let doubleClickInterval: @MainActor () -> TimeInterval
  /// The window takes the keyboard.
  @ObservationIgnored private(set) var focused = true
  /// Numbers each acquisition and release, so a late reply knows it was overtaken.
  @ObservationIgnored private(set) var controlGeneration = 0
  /// The acquisition that waits for a fresh picture, Take Control having resumed a paused one; nil while none waits.
  @ObservationIgnored private(set) var pictureWait: Int?
  /// Ends that wait once it has gone on too long.
  @ObservationIgnored private var pictureWaitEnd: Task<Void, Never>?
  /// The status reading under way when the lease was granted: only a later one can say the lease has ended.
  @ObservationIgnored private(set) var controlSince = 0
  /// Renews the lease and ends control over a stale picture.
  @ObservationIgnored private(set) var heartbeat: Task<Void, Never>?
  /// The mouse button held and the clicks counted.
  @ObservationIgnored private var gesture = PointerGesture()
  /// Scrolling gathered and not yet sent.
  @ObservationIgnored private var wheel = WheelAccumulator()
  /// Where the gathered scrolling happens.
  @ObservationIgnored private var wheelAt: DesktopPoint?
  /// When the last scroll event came, in seconds of uptime; a long gap starts a new gesture.
  @ObservationIgnored private var wheelHeardAt: TimeInterval?
  /// Sends the gathered scrolling once the gathering ends.
  @ObservationIgnored private var wheelSending: Task<Void, Never>?
  /// The input waiting to go to the helper, in order.
  @ObservationIgnored private let input = InputQueue()
  /// When input last asked for a reading, in seconds of uptime.
  @ObservationIgnored private var inputReadAt: TimeInterval?
  /// Asks for the reading input asked for too soon after the last, once its pause ends.
  @ObservationIgnored private(set) var inputReading: Task<Void, Never>?

  /// Creates control for `viewer`, holding nothing.
  ///
  /// - Parameters:
  ///   - viewer: The window it belongs to.
  ///   - dependencies: What it reaches outside the window.
  init(viewer: ViewerController, dependencies: ViewerDependencies) {
    self.viewer = viewer
    service = dependencies.service
    actions = dependencies.actions
    uptime = dependencies.uptime
    clock = dependencies.clock
    heartbeatInterval = dependencies.heartbeatInterval
    doubleClickInterval = dependencies.doubleClickInterval
    input.sink = self
  }

  /// Take Control from any tab: the Screen tab shows first, since control acts on the picture. Pressed while the
  /// picture is paused, it resumes the picture first and takes control once a fresh one arrives.
  func takeControlOnScreen() async {
    if let viewer, viewer.session.tab != .screen { viewer.selectTab(.screen) }
    await takeControl()
  }

  /// Take control with the picture on screen. The attempt is reserved before anything is awaited, so leaving the
  /// window or the tab meanwhile cancels it, and it is checked again after every wait. A release the service refused
  /// before is sent again first, outside the attempt's own control work. The request and its reply count as control
  /// work under way (``ControlBarrier``), so the helper's commands wait for it; a grant that arrives after the attempt
  /// was cancelled, or once one of the helper's commands started, is given back before that work ends, and a reply
  /// that was lost holds the barrier until a lease it may have granted can no longer be alive. A busy service is
  /// asked again every ``busyRetryAfter``, up to ``busyRetries`` times, the button saying Connecting… meanwhile; still
  /// busy, a toast says the agent didn't finish its step and the button shakes (``ViewerController/refuse(_:)``).
  /// With the picture paused, it resumes it and waits for a fresh one first (``awaitFreshPicture()``).
  func takeControl() async {
    guard let viewer else { return }
    if viewer.session.paused {
      if viewer.snapshot.canTakeControlOnceResumed { awaitFreshPicture() }
      return
    }
    guard viewer.snapshot.canTakeControl else { return }
    acquiring = true
    controlGeneration += 1
    let generation = controlGeneration
    if !barrier.unreleased.isEmpty {
      let refused = await barrier.settle()
      // Cancelled while it waited: releaseControl moved the generation on and cleared the attempt.
      guard generation == controlGeneration else { return }
      if let refused {
        acquiring = false
        return viewer.say(refused)
      }
    }
    // The live clock, not the last tick's: a picture gone stale since then is not one to aim at.
    guard accepts(generation), !barrier.isBusy, !pictureStale, let frame = viewer.reading.frame else {
      if generation == controlGeneration { acquiring = false }
      return
    }
    do {
      let accepted = try await barrier.tracking {
        let granted = try await acquireOrHold(frame: frame.id, generation: generation)
        guard accepts(generation) else {
          _ = await barrier.releaseNow(granted)
          return false
        }
        accept(granted)
        return true
      }
      if !accepted, generation == controlGeneration { acquiring = false }
    } catch {
      guard generation == controlGeneration else { return }
      acquiring = false
      let busy = (error as? ServiceError)?.isBusy == true
      if busy { viewer.refuse(ViewerWords.agentMidAction) } else { viewer.say(error.localizedDescription) }
    }
  }

  /// Asks for control, holding the barrier past its end when the outcome is unknown: only the service's own refusal,
  /// which carries its code, says for certain that no lease was granted; a lost, late or cancelled reply may have
  /// granted one.
  ///
  /// - Parameters:
  ///   - frame: The picture the person acts on.
  ///   - generation: The acquisition's number.
  /// - Returns: The lease granted.
  /// - Throws: The service's refusal, or the failure that left the outcome unknown.
  private func acquireOrHold(frame: String, generation: Int) async throws -> String {
    do {
      return try await acquire(frame: frame, generation: generation)
    } catch {
      if (error as? ServiceError)?.code == nil { barrier.holdLostAcquisition() }
      throw error
    }
  }

  /// Whether a grant for acquisition `generation` is still wanted: nothing newer happened, the window shows the
  /// Screen tab with a ready, unpaused picture and takes the keyboard, and none of the helper's commands runs.
  ///
  /// - Parameter generation: The acquisition's number.
  /// - Returns: Whether to keep the grant; never once the window is gone.
  private func accepts(_ generation: Int) -> Bool {
    guard let viewer else { return false }
    return generation == controlGeneration && focused && viewer.shown && !viewer.session.paused
      && viewer.reading.ready && viewer.session.tab == .screen && !actions.isBusy && !viewer.session.alertRunning
  }

  /// Holds `granted`: input goes, the heartbeat starts, and the next reading comes at once.
  ///
  /// - Parameter granted: The lease granted.
  private func accept(_ granted: String) {
    lease = granted
    // A status read under way now was asked before the grant, so it cannot know of this lease.
    controlSince = viewer?.readingLoop.statusReading ?? 0
    acquiring = false
    input.activate(granted)
    viewer?.say(nil)
    viewer?.dismissToast()
    startHeartbeat(granted)
    viewer?.readingLoop.refresh()
  }

  /// Asks for control over `frame`, and again over the newest picture every ``busyRetryAfter`` while the service
  /// answers busy, up to ``busyRetries`` times, unless the person moved on meanwhile.
  ///
  /// - Parameters:
  ///   - frame: The picture the person acts on.
  ///   - generation: The acquisition's number; a newer one means the person moved on.
  /// - Returns: The lease granted.
  /// - Throws: The service's refusal: the second busy one, or the first when the picture went stale meanwhile.
  private func acquire(frame: String, generation: Int) async throws -> String {
    var asked = frame
    var retries = 0
    while true {
      do {
        return try await service.acquire(frame: asked)
      } catch let refusal as ServiceError where refusal.isBusy {
        guard retries < Self.busyRetries else { throw refusal }
        retries += 1
        // A wait cut short only retries sooner; the guard below drops a retry the person moved on from.
        try? await clock.sleep(for: Self.busyRetryAfter)
        guard generation == controlGeneration, let newest = viewer?.reading.frame, !pictureStale else { throw refusal }
        asked = newest.id
      }
    }
  }

  /// Take Control pressed while the picture is paused: the picture resumes, which gives back nothing, as nothing is
  /// held, and forgets the old frame; the button says Connecting… while the window waits for a fresh picture, which
  /// takes control as it arrives (``pictureArrived()``). Leaving the tab or the window, pausing again or Give Back
  /// ends the wait, as they end any acquisition (``releaseControl(_:)``); one that lasts ``freshPictureWait`` gives
  /// up, saying so.
  private func awaitFreshPicture() {
    viewer?.togglePause()
    acquiring = true
    controlGeneration += 1
    let generation = controlGeneration
    pictureWait = generation
    // Unstructured, as the wait ends by itself while the window goes on; a newer acquisition or a release ends it.
    pictureWaitEnd = Task { [weak self, clock] in
      try? await clock.sleep(for: Self.freshPictureWait)
      guard let self, !Task.isCancelled, pictureWait == generation else { return }
      endPictureWait()
      acquiring = false
      self.viewer?.say(ViewerWords.noFreshPicture)
    }
  }

  /// A fresh picture arrived: the acquisition that waited for it goes on, as Take Control pressed now would.
  func pictureArrived() {
    guard let waited = pictureWait, waited == controlGeneration else { return }
    endPictureWait()
    acquiring = false
    // Unstructured, as a picture lands synchronously; Take Control checks everything again before it asks.
    Task { await takeControl() }
  }

  /// Ends the wait for a fresh picture, if one waits.
  private func endPictureWait() {
    pictureWait = nil
    pictureWaitEnd?.cancel()
    pictureWaitEnd = nil
  }

  /// Give control back: at once here, then to the service through the app's barrier, which counts the release until
  /// the service answers. Also cancels an acquisition still under way, whose late grant is then given back.
  ///
  /// - Parameter message: What the notice says, if anything.
  func releaseControl(_ message: String? = nil) {
    controlGeneration += 1
    let generation = controlGeneration
    let previous = lease
    lease = nil
    acquiring = false
    endPictureWait()
    heartbeat?.cancel()
    heartbeat = nil
    clearInput()
    if let previous {
      barrier.release(previous) { [weak self] refusal in
        guard let self, let viewer, generation == controlGeneration, lease == nil else { return }
        if let refusal { return viewer.say(refusal) }
        // Show the release at once: reading the status overtakes a reading under way. Unstructured, as the barrier
        // answers through a synchronous callback.
        Task {
          await viewer.readingLoop.readStatus()
          viewer.readingLoop.refresh()
        }
      }
    }
    if let message { viewer?.say(message) }
  }

  /// Drops every input waiting, the gathered scrolling, the gesture under way and the text being composed, whose
  /// chip would otherwise stay once control ends.
  private func clearInput() {
    input.activate(nil)
    composing = ""
    wheel.reset()
    gesture.cancel()
    wheelAt = nil
    wheelHeardAt = nil
    wheelSending?.cancel()
    wheelSending = nil
    inputReading?.cancel()
    inputReading = nil
  }

  /// Renews the lease now and then, and ends control once the picture is stale, whether or not the person is still
  /// giving input.
  ///
  /// - Parameter granted: The lease just granted.
  private func startHeartbeat(_ granted: String) {
    heartbeat = Task { [weak self, clock] in
      var tick = 0
      while !Task.isCancelled {
        guard let interval = self?.heartbeatInterval else { return }
        // A cancelled heartbeat ends at once; the guard below ends the loop.
        try? await clock.sleep(for: interval)
        guard !Task.isCancelled, let self, lease == granted else { return }
        guard !pictureStale else {
          releaseControl(ViewerWords.stalePicture)
          return
        }
        tick += 1
        if tick % Self.renewEvery == 0 { await renew(granted) }
      }
    }
  }

  /// Renews the lease, unless it has ended; a failed renewal ends control.
  ///
  /// - Parameter owner: The lease to renew.
  private func renew(_ owner: String) async {
    guard lease == owner, let frame = viewer?.reading.frame else { return }
    do {
      try await service.renew(lease: owner, frame: frame.id)
    } catch {
      if lease == owner { releaseControl(error.localizedDescription) }
    }
  }

  /// The window stopped taking the keyboard: control goes back.
  func lostFocus() {
    focused = false
    releaseControl()
  }

  /// The window takes the keyboard again; control waits to be taken.
  func gainedFocus() {
    focused = true
  }

  /// The last picture is too old to aim input at; with no window, there is none to aim at.
  var pictureStale: Bool {
    guard let frameAt = viewer?.reading.frameAt else { return true }
    return uptime() - frameAt >= Freshness.stale
  }

  /// Whether input may go to the helper now. Only the heartbeat ends control over a stale picture; this side just
  /// stops sending.
  var canInput: Bool {
    guard let viewer else { return false }
    return lease != nil && viewer.reading.frame != nil && !viewer.session.paused && focused && viewer.shown
      && !pictureStale
  }

  /// Queues input for the helper, aimed at the picture on screen, while input may go.
  ///
  /// - Parameter actions: The actions, in order.
  private func enqueue(_ actions: [InputAction]) {
    guard canInput, let frame = viewer?.reading.frame else { return }
    input.enqueue(actions, frame: frame.id)
  }

  /// Queues input that undoes what earlier input holds down, a button's release, whenever control is held: over a
  /// stale picture too, which the service may refuse, and then control ends. With no picture to aim at, control
  /// ends at once, and the service's release lifts what is held.
  ///
  /// - Parameter actions: The actions, in order.
  private func enqueueCleanup(_ actions: [InputAction]) {
    guard lease != nil else { return }
    guard let frame = viewer?.reading.frame else { return releaseControl() }
    input.enqueue(actions, frame: frame.id)
  }

  /// The display point under `location` in a view of `size`.
  ///
  /// - Parameters:
  ///   - location: The point in the view, from its top left.
  ///   - size: The view's size.
  ///   - clamp: Whether a point outside the picture moves to its nearest edge.
  /// - Returns: The point on the helper's display, or nil when there is none.
  func point(_ location: CGPoint, in size: CGSize, clamp: Bool = false) -> DesktopPoint? {
    guard let reading = viewer?.reading, let frame = reading.frame, let pictureSize = reading.pictureSize else {
      return nil
    }
    return DesktopPoint(location: location, in: size, image: pictureSize, display: frame.display, clamp: clamp)
  }

  /// View points per point of the helper's display in a view of `size`; 1 with no picture to measure by.
  ///
  /// - Parameter size: The view's size.
  /// - Returns: The view's scale.
  func scale(in size: CGSize) -> Double {
    guard let reading = viewer?.reading, let frame = reading.frame, let pictureSize = reading.pictureSize else {
      return 1
    }
    return DesktopPoint.scale(in: size, image: pictureSize, display: frame.display) ?? 1
  }

  /// A button went down over the desktop.
  ///
  /// - Parameters:
  ///   - button: The button.
  ///   - location: Where, in the view.
  ///   - size: The view's size.
  ///   - modifiers: The modifier keys held.
  func pointerDown(_ button: MouseButton, at location: CGPoint, in size: CGSize, modifiers: Modifiers) {
    guard canInput, let point = point(location, in: size),
      let action = gesture.start(
        button,
        at: point,
        scale: scale(in: size),
        modifiers: modifiers,
        now: uptime(),
        repeatWithin: doubleClickInterval()
      )
    else { return }
    enqueue([action])
  }

  /// A held button moved.
  ///
  /// - Parameters:
  ///   - button: The button.
  ///   - location: Where, in the view.
  ///   - size: The view's size.
  ///   - modifiers: The modifier keys held.
  func pointerDragged(_ button: MouseButton, to location: CGPoint, in size: CGSize, modifiers: Modifiers) {
    guard canInput, let point = point(location, in: size, clamp: true),
      let action = gesture.move(button, to: point, scale: scale(in: size), modifiers: modifiers)
    else { return }
    enqueue([action])
  }

  /// The button comes up where it is, or where the gesture last was. The release is cleanup, so it goes whenever
  /// control is held, over a stale picture too: dropped, it would leave the helper's button down, and later moves
  /// would drag.
  ///
  /// - Parameters:
  ///   - button: The button.
  ///   - location: Where, in the view.
  ///   - size: The view's size.
  ///   - modifiers: The modifier keys held.
  func pointerUp(_ button: MouseButton, at location: CGPoint, in size: CGSize, modifiers: Modifiers) {
    guard let point = point(location, in: size, clamp: true) ?? gesture.last else { return }
    let movement = gesture.move(button, to: point, scale: scale(in: size), modifiers: modifiers)
    guard let action = gesture.finish(button, at: point, modifiers: modifiers, now: uptime()) else { return }
    enqueueCleanup([movement, action].compactMap { $0 })
  }

  /// The pointer moved with no button held.
  ///
  /// - Parameters:
  ///   - location: Where, in the view.
  ///   - size: The view's size.
  func pointerMoved(to location: CGPoint, in size: CGSize) {
    guard canInput, !gesture.isActive, let point = point(location, in: size) else { return }
    enqueue([.move(x: point.x, y: point.y)])
  }

  /// Scrolling is gathered for a moment and sent as whole lines. A new gesture, one the trackpad says begins or one
  /// after a wheel's pause of ``newScrollAfter``, drops the fraction of a line the last one left. While a scroll
  /// still waits to be sent, more lines gather here instead of queueing behind it.
  ///
  /// - Parameters:
  ///   - dx: AppKit's horizontal delta.
  ///   - dy: AppKit's vertical delta.
  ///   - precise: Whether it came from a trackpad, in points, rather than a wheel's lines.
  ///   - began: Whether the trackpad says this event begins a gesture.
  ///   - location: Where, in the view.
  ///   - size: The view's size.
  func scrolled(dx: Double, dy: Double, precise: Bool, began: Bool, at location: CGPoint, in size: CGSize) {
    guard canInput, let point = point(location, in: size) else { return }
    let now = uptime()
    let paused = wheelHeardAt.map { Duration.seconds(now - $0) > Self.newScrollAfter } ?? true
    if began || (!precise && paused) { wheel.beginGesture() }
    wheelHeardAt = now
    wheel.add(dx: dx, dy: dy, precise: precise)
    wheelAt = point
    sendWheelSoon()
  }

  /// Sends the gathered scrolling once the gathering ends, unless a send already waits; while a scroll batch is still
  /// queued, it waits another gathering, so steady scrolling never piles batches up.
  private func sendWheelSoon() {
    guard wheelSending == nil else { return }
    wheelSending = Task { [weak self, clock] in
      // A cancelled gathering ends at once; the guard below sends nothing.
      try? await clock.sleep(for: Self.scrollGathering)
      guard let self, !Task.isCancelled else { return }
      wheelSending = nil
      if input.holdsScroll { return sendWheelSoon() }
      sendWheel()
    }
  }

  /// Sends the whole lines gathered; lines past one send's most go with the next, which is not left waiting for
  /// another scroll event.
  private func sendWheel() {
    let delta = wheel.take()
    guard let at = wheelAt, delta.dx != 0 || delta.dy != 0 else { return }
    // AppKit's positive delta moves content down (towards the top of
    // the page); Atlas's positive lines scroll down the page.
    enqueue([
      .move(x: at.x, y: at.y), .wait(milliseconds: Self.scrollSettle),
      .scroll(x: at.x, y: at.y, dx: -delta.dx, dy: -delta.dy),
    ])
    if wheel.holdsWholeLines { sendWheelSoon() }
  }

  /// A key press: release, paste, a key, or nil when it is text for the
  /// input method. Returns whether it was handled.
  ///
  /// - Parameters:
  ///   - keyCode: The key's virtual key code.
  ///   - modifiers: The modifier keys held.
  ///   - clipboard: This Mac's clipboard text, read only for Command-V.
  /// - Returns: Whether it was handled here.
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

  /// A key from the footer's Keys menu (Escape, a shortcut).
  ///
  /// - Parameter key: The key, as the service names it.
  func sendKey(_ key: String) {
    enqueue([.key(key)])
  }

  /// Committed text, typed in order, in chunks the helper accepts; text it would refuse is not sent, and the notice
  /// says why.
  ///
  /// - Parameter text: The text.
  func send(text: String) {
    guard !text.isEmpty, canInput else { return }
    switch TypedText(text) {
    case .chunks(let chunks):
      for chunk in chunks { enqueue([.type(text: chunk)]) }
    case .tooLong:
      viewer?.say(ViewerWords.pasteTooLong(TypedText.pasteLimit))
    case .sequenceTooLong:
      viewer?.say(ViewerWords.sequenceTooLong)
    }
  }

  /// What the input method is composing now, shown on the screen until it is committed or ends. The same text again
  /// changes nothing, so the window does not redraw for it.
  ///
  /// - Parameter text: The composed text; empty once the composition ends.
  func compose(_ text: String) {
    guard composing != text else { return }
    composing = text
  }
}

extension ViewerControl: InputSink {
  /// Sends queued input to the service, counted as control work under way (``ControlBarrier``) until it answers: the
  /// person's own batch still in flight after Give Back holds the helper, so Take Control waits for it rather than
  /// meeting the service's busy answer and blaming an agent.
  ///
  /// - Parameters:
  ///   - lease: The lease the input belongs to.
  ///   - frame: The picture it was aimed at.
  ///   - actions: The actions, in order.
  /// - Throws: The service's error when it refuses the input or does not answer.
  func send(lease: String, frame: String, actions: [InputAction]) async throws {
    try await barrier.tracking { try await service.input(lease: lease, frame: frame, actions: actions) }
  }

  /// Input went: read again, to show its effect. At once after a quiet spell; while input keeps coming, as a moving
  /// mouse sends it, at most once each ``inputReadEvery``.
  func inputSent() {
    guard inputReading == nil else { return }
    let now = uptime()
    let wait = inputReadAt.map { Self.inputReadEvery - .seconds(now - $0) } ?? .zero
    guard wait > .zero else {
      inputReadAt = now
      viewer?.readingLoop.refresh()
      return
    }
    inputReading = Task { [weak self, clock] in
      // A cancelled wait ends at once; the guard below reads nothing.
      try? await clock.sleep(for: wait)
      guard let self, !Task.isCancelled else { return }
      inputReading = nil
      inputReadAt = uptime()
      viewer?.readingLoop.refresh()
    }
  }

  /// Input failed: control ends, saying why.
  ///
  /// - Parameter error: Why.
  func inputFailed(_ error: any Error) { releaseControl(error.localizedDescription) }
}
