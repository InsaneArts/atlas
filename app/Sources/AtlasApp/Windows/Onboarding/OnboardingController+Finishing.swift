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

  /// Try Again after a failed check.
  func retryFinish() {
    guard !isQueued(.check) else { return }
    snapshot.finish = .idle
    runFinish()
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

  /// Done: set Open at Login, open Atlas, then close the window; on any failure the window stays, saying why.
  func complete() async {
    guard let login = await act(.setOpensAtLogin(snapshot.opensAtLogin), OnboardingWords.finishing),
      login.ok,
      let started = await act(.openMenuBar, OnboardingWords.opening),
      started.ok
    else { return }
    closeRequested = true
  }
}
