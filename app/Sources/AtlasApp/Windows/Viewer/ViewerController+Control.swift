import AtlasService
import AtlasStatus
import Foundation

/// Control: the lease this window holds, its heartbeat, its release, and the input that belongs to it. The service
/// checks every action against the lease and the picture it was aimed at; this side only stops sending early, and
/// waits for every release to be answered before control can be taken again.
extension ViewerController {
  /// The lease is renewed every this many heartbeat ticks: 5, every 2.5 s at ``heartbeatEvery``.
  static let renewEvery = 5
  /// How long Take Control waits before asking once more after the service answers busy: 1 s. The service is busy
  /// while one of an agent's actions runs, and a click or a short burst of keys ends well within a second; a longer
  /// action is not worth holding the button for, so the person is told to try again instead.
  static let busyRetryAfter: Duration = .seconds(1)

  /// Take Control from any tab: the Screen tab shows first, since control acts on the picture.
  func takeControlOnScreen() async {
    if session.tab != .screen { selectTab(.screen) }
    await takeControl()
  }

  /// Take control with the picture on screen. The attempt is reserved before anything is awaited, so leaving the
  /// window or the tab meanwhile cancels it, and it is checked again after every wait. A release the service refused
  /// before is sent again first, outside the attempt's own control work. The request and its reply count as control
  /// work under way (``ControlBarrier``), so the helper's commands wait for it; a grant that arrives after the attempt
  /// was cancelled, or once one of the helper's commands started, is given back before that work ends, and a reply
  /// that was lost holds the barrier until a lease it may have granted can no longer be alive. A busy service is
  /// asked once more after ``busyRetryAfter``; still busy, the notice says the agent is mid-action.
  func takeControl() async {
    guard snapshot.canTakeControl else { return }
    session.acquiring = true
    controlGeneration += 1
    let generation = controlGeneration
    if !barrier.unreleased.isEmpty {
      let refused = await barrier.settle()
      // Cancelled while it waited: releaseControl moved the generation on and cleared the attempt.
      guard generation == controlGeneration else { return }
      if let refused {
        session.acquiring = false
        return say(refused)
      }
    }
    // The live clock, not the last tick's: a picture gone stale since then is not one to aim at.
    guard accepts(generation), !barrier.isBusy, !pictureStale, let frame = reading.frame else {
      if generation == controlGeneration { session.acquiring = false }
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
      if !accepted, generation == controlGeneration { session.acquiring = false }
    } catch {
      guard generation == controlGeneration else { return }
      session.acquiring = false
      let busy = (error as? ServiceError)?.isBusy == true
      say(busy ? ViewerWords.agentMidAction : error.localizedDescription)
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
  /// - Returns: Whether to keep the grant.
  private func accepts(_ generation: Int) -> Bool {
    generation == controlGeneration && focused && shown && !session.paused && reading.ready
      && session.tab == .screen && !actions.isBusy && !session.alertRunning
  }

  /// Holds `granted`: input goes, the heartbeat starts, and the next reading comes at once.
  ///
  /// - Parameter granted: The lease granted.
  private func accept(_ granted: String) {
    session.lease = granted
    // A status read under way now was asked before the grant, so it cannot know of this lease.
    controlSince = statusReading
    session.acquiring = false
    input.activate(granted)
    say(nil)
    startHeartbeat(granted)
    refresh()
  }

  /// Asks for control over `frame`, and once more over the newest picture after ``busyRetryAfter`` when the service
  /// answers busy, unless the person moved on meanwhile.
  ///
  /// - Parameters:
  ///   - frame: The picture the person acts on.
  ///   - generation: The acquisition's number; a newer one means the person moved on.
  /// - Returns: The lease granted.
  /// - Throws: The service's refusal: the second busy one, or the first when the picture went stale meanwhile.
  private func acquire(frame: String, generation: Int) async throws -> String {
    do {
      return try await service.acquire(frame: frame)
    } catch let refusal as ServiceError where refusal.isBusy {
      await nap(Self.busyRetryAfter)
      guard generation == controlGeneration, let newest = reading.frame, !pictureStale else { throw refusal }
      return try await service.acquire(frame: newest.id)
    }
  }

  /// Give control back: at once here, then to the service through the app's barrier, which counts the release until
  /// the service answers. Also cancels an acquisition still under way, whose late grant is then given back.
  ///
  /// - Parameter message: What the notice says, if anything.
  func releaseControl(_ message: String? = nil) {
    controlGeneration += 1
    let generation = controlGeneration
    let previous = session.lease
    session.lease = nil
    session.acquiring = false
    heartbeat?.cancel()
    heartbeat = nil
    clearInput()
    if let previous {
      barrier.release(previous) { [weak self] refusal in
        guard let self, generation == controlGeneration, session.lease == nil else { return }
        if let refusal { return say(refusal) }
        // Show the release at once: reading the status overtakes a reading under way.
        Task {
          await self.readStatus()
          self.refresh()
        }
      }
    }
    if let message { say(message) }
  }

  /// Drops every input waiting, the gathered scrolling, the gesture under way and the text being composed, whose
  /// chip would otherwise stay once control ends.
  func clearInput() {
    input.activate(nil)
    session.composing = ""
    wheel.reset()
    gesture.cancel()
    wheelAt = nil
    wheelHeardAt = nil
    wheelSending?.cancel()
    wheelSending = nil
    inputReading?.cancel()
    inputReading = nil
  }

  /// Stop or Resume, saying how it went, then reads the status so the window shows it at once.
  ///
  /// - Parameter stopped: Whether agents stop.
  func setStopped(_ stopped: Bool) async {
    do {
      try await actions.setStopped(stopped)
      say(stopped ? ViewerWords.agentStopped : ViewerWords.agentResumed)
    } catch {
      say(error.localizedDescription)
    }
    await readStatus()
    refresh()
  }

  /// Renews the lease now and then, and ends control once the picture is stale, whether or not the person is still
  /// giving input.
  ///
  /// - Parameter granted: The lease just granted.
  func startHeartbeat(_ granted: String) {
    heartbeat = Task { [weak self] in
      var tick = 0
      while !Task.isCancelled {
        guard let interval = self?.heartbeatInterval else { return }
        try? await Task.sleep(for: interval)
        guard !Task.isCancelled, let self, session.lease == granted else { return }
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
    guard session.lease == owner, let frame = reading.frame else { return }
    do {
      try await service.renew(lease: owner, frame: frame.id)
    } catch {
      if session.lease == owner { releaseControl(error.localizedDescription) }
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

  /// The last picture is too old to aim input at.
  var pictureStale: Bool {
    guard let frameAt = reading.frameAt else { return true }
    return clock() - frameAt >= Freshness.stale
  }

  /// Whether input may go to the helper now. Only the heartbeat ends control over a stale picture; this side just
  /// stops sending.
  var canInput: Bool {
    session.lease != nil && reading.frame != nil && !session.paused && focused && shown && !pictureStale
  }

  /// Queues input for the helper, aimed at the picture on screen, while input may go.
  ///
  /// - Parameter actions: The actions, in order.
  func enqueue(_ actions: [InputAction]) {
    guard canInput, let frame = reading.frame else { return }
    input.enqueue(actions, frame: frame.id)
  }

  /// Queues input that undoes what earlier input holds down, a button's release, whenever control is held: over a
  /// stale picture too, which the service may refuse, and then control ends. With no picture to aim at, control
  /// ends at once, and the service's release lifts what is held.
  ///
  /// - Parameter actions: The actions, in order.
  func enqueueCleanup(_ actions: [InputAction]) {
    guard session.lease != nil else { return }
    guard let frame = reading.frame else { return releaseControl() }
    input.enqueue(actions, frame: frame.id)
  }
}
