import AtlasCommands

/// The queue: the window's requests, performed in order by ``OnboardingController/run()``, one at a time, beside the
/// reading loop.
extension OnboardingController {
  /// Performs queued requests in order, one at a time, until the window closes.
  ///
  /// Once the window has closed, a request still waiting is dropped, and the queue ends after the one under way has
  /// settled. A request Back dropped while it waited, or once taken and before it started, never runs.
  func performRequests() async {
    for await ticket in requests {
      // Dropped by Back while it waited, or the window closed: it never runs.
      guard let request = pending.removeValue(forKey: ticket), !closed else { continue }
      // Unstructured, so Back can stop the request under way without stopping the queue, which waits for it all the
      // same; cancelling `run()` stops it too.
      let work = Task {
        // Its start is a main-actor job of its own, so Back or the close can come between the taking and the start:
        // then it never runs.
        guard !self.closed, !Task.isCancelled else { return }
        await self.perform(request)
      }
      performing = (request, work)
      await withTaskCancellationHandler {
        await work.value
      } onCancel: {
        work.cancel()
      }
      performing = nil
    }
  }

  /// Queues a request, unless the same one is already waiting or under way, or the window has closed. A reading is
  /// never queued: the reading loop takes it at once, so it waits behind no command.
  ///
  /// - Parameter request: The request.
  func enqueue(_ request: OnboardingRequest) {
    guard !closed else { return }
    guard request != .refresh else { return refresh() }
    guard !isQueued(request) else { return }
    lastTicket += 1
    pending[lastTicket] = request
    queue.yield(lastTicket)
  }

  /// Whether a request that does what `request` does is waiting in the queue or under way
  /// (``OnboardingRequest/repeats(_:)``).
  ///
  /// - Parameter request: The request.
  /// - Returns: Whether one is.
  func isQueued(_ request: OnboardingRequest) -> Bool {
    pending.values.contains { request.repeats($0) } || performing.map { request.repeats($0.request) } == true
  }

  /// Drops the waiting requests that `ends` picks, and stops the one under way when it picks that too.
  ///
  /// - Parameter ends: Whether a request ends.
  func stop(where ends: (OnboardingRequest) -> Bool) {
    pending = pending.filter { !ends($0.value) }
    if let performing, ends(performing.request) { performing.work.cancel() }
  }

  /// Performs one request.
  ///
  /// - Parameter request: The request.
  private func perform(_ request: OnboardingRequest) async {
    switch request {
    case .useHelper: await useHelper()
    case .createAccount: await createAccount()
    case .lockAndSwitch: await act(.switchTo(snapshot.facts.helper.name), Words.locking)
    case .chooseAnother: await chooseAnother()
    case .connectAgents(let chosen): await connectAgents(chosen)
    case .complete: await complete()
    case .allow: await allow()
    case .switchBack: await switchBack()
    case .refresh: refresh()
    case .check: await check()
    case .readAgents: await readAgents()
    case .openSettings(let permission): openSettings(permission)
    case .moveOn(let permission): await moveOn(from: permission)
    }
  }
}
