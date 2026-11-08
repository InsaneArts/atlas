import AtlasCommands

/// The end of the person's side: the end-to-end check, then Done.
extension OnboardingController {
  /// Queues the end-to-end check; never a second one while one waits or runs, and none once the window has closed.
  func runFinish() {
    guard !closed, !isQueued(.check) else { return }
    snapshot.finish = .running
    snapshot.passSettled = false
    enqueue(.check)
  }

  /// Try Again after a failed check: it starts as the first did, on the next reading, asked for at once, once this
  /// login has been read on screen ``settledReadings`` times in a row.
  func retryFinish() {
    guard !isQueued(.check) else { return }
    snapshot.finish = .idle
    refresh()
  }

  /// Runs the end-to-end check, then reads the facts again; a check the closing window stopped shows nothing. Once it
  /// passed, its Continue lights up and takes presses ``continueFollows`` later.
  func check() async {
    let outcome = await commands.run(.finishSetup)
    guard !closed else { return }
    snapshot.finish = outcome.ok ? .passed : .failed(outcome.message)
    refresh()
    guard outcome.ok else { return }
    // Cut short only as `run()` ends, which drops what follows.
    try? await dependencies.clock.sleep(for: Self.continueFollows)
    guard !closed, !Task.isCancelled, snapshot.finish == .passed else { return }
    snapshot.passSettled = true
  }

  /// Done: set Open at Login, keep the home folder private as the box says, open Atlas, then close the window; on any
  /// failure the window stays, saying why. A home folder Atlas couldn't make private stops this Done there, with the
  /// box turned off, and the next Done finishes without it.
  func complete() async {
    guard let login = await act(.setOpensAtLogin(snapshot.opensAtLogin), OnboardingWords.finishing),
      login.ok,
      await keepHomePrivate(),
      let started = await act(.openMenuBar, OnboardingWords.opening),
      started.ok
    else { return }
    closeRequested = true
  }

  /// Makes the home folder private when the box is checked, else remembers that it was offered, unless an attempt
  /// failed: then it stays unoffered, so Atlas offers it again later.
  ///
  /// - Returns: Whether Done goes on; false after a failed attempt, which says why and turns the box off.
  func keepHomePrivate() async -> Bool {
    guard snapshot.keepsHomePrivate else {
      if !snapshot.homePrivacyFailed { _ = await commands.run(.homeOffered) }
      return true
    }
    guard let made = await act(.makeHomePrivate, OnboardingWords.makingPrivate) else { return false }
    guard made.ok else {
      snapshot.homePrivacyFailed = true
      snapshot.keepsHomePrivate = false
      snapshot.problem = OnboardingWords.homeNotPrivate(made.message)
      return false
    }
    return true
  }
}
