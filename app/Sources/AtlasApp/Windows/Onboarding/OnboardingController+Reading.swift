import AtlasCommands
import AtlasOnboarding
import AtlasUI

/// The reading loop: the facts read now and every ``OnboardingController/readEvery``, one reading at a time, beside the
/// queue, so a reading never waits behind a command.
extension OnboardingController {
  /// Reads the facts, then pauses ``readEvery``, until the window closes; a refresh cuts the pause short.
  func readFacts() async {
    while !closed, !Task.isCancelled {
      readAgain = false
      await read()
      // A close during the reading ends the loop here, with no pause first.
      if !readAgain, !closed { await rest() }
    }
  }

  /// Reads the facts again soon: now, or right after the reading under way.
  func refresh() {
    guard !closed else { return }
    readAgain = true
    resting?.cancel()
  }

  /// The pause between readings, on the clock, until it ends, a refresh or the close cuts it short, or the loop is
  /// cancelled.
  private func rest() async {
    // Unstructured, so a refresh or the window's close can cut the pause short; the loop waits for it.
    let pause = Task<Void, Never> { [clock = dependencies.clock] in try? await clock.sleep(for: Self.readEvery) }
    resting = pause
    await withTaskCancellationHandler {
      await pause.value
    } onCancel: {
      pause.cancel()
    }
    resting = nil
  }

  /// One reading. One that ends after the window closed is dropped.
  private func read() async {
    let outcome = await commands.run(.onboardingStatus)
    guard !closed else { return }
    if outcome.ok, var facts = OnboardingFacts(json: outcome.reply) {
      // A job restarting (to pick up Screen Recording) leaves its permissions unknown, never off.
      readingsUnknown = facts.asker.permissionsKnown ? 0 : readingsUnknown + 1
      facts.asker = facts.asker.keepingLastKnown(
        of: snapshot.facts.asker,
        restarting: readingsUnknown <= Self.restartReadings
      )
      snapshot.facts = facts
      if !snapshot.loaded {
        snapshot.opensAtLogin = facts.opensAtLogin(enabledNow: dependencies.loginItem.isEnabled())
        // A failed first reading's message goes with it.
        snapshot.problem = nil
      }
      snapshot.loaded = true
      // A pick the choice no longer shows, or none yet, gives way to the default.
      if snapshot.shownPick == nil { snapshot.picked = facts.defaultPick.map(OnboardingAccountPick.init(name:)) }
      await advance()
    } else if !snapshot.loaded {
      snapshot.problem = outcome.ok ? OnboardingWords.notUnderstood : Words.sentence(outcome.message)
    }
  }

  /// Starts what a step needs as soon as it is shown, moves the permission panel on once its permission is on, and
  /// marks the helper's window open.
  private func advance() async {
    guard !closed else { return }
    let step = snapshot.step
    switch step {
    case .finishing where snapshot.finish == .idle: runFinish()
    case .agents where snapshot.agents.isEmpty: loadAgents()
    case .agents where snapshot.agentsFailed: retryAgents()
    default: break
    }
    // A check's outcome holds for this visit to the step alone: coming back runs it again.
    if step != .finishing, snapshot.finish != .running { snapshot.finish = .idle }
    if let permission = snapshot.guiding {
      // With both on the step is the helper's done screen already, while the last tick still shows.
      if step != .permissions, step != .helperDone {
        snapshot.guiding = nil
      } else if permission.granted(to: snapshot.facts.asker) {
        enqueue(.moveOn(from: permission))
      }
    }
    switch step {
    // In the helper the window marks itself open even when blocked: the opener waits for that mark.
    case .allow, .permissions, .helperDone, .blocked: await markWindow(open: true)
    default: break
    }
  }

  /// While open in the helper, Atlas's own "set up here" notice stays quiet; tried again on the next reading when it
  /// fails. The reading loop waits for it, so one mark runs at a time, and a close meanwhile clears the mark once
  /// this one has settled.
  ///
  /// - Parameter open: Whether the window is open.
  private func markWindow(open: Bool) async {
    guard snapshot.facts.role == .helper, !closed, open != markedOpen else { return }
    marking = true
    let marked = await commands.run(.markWindow(open: open)).ok
    marking = false
    markSettled?.resume()
    markSettled = nil
    if marked { markedOpen = open }
  }
}
