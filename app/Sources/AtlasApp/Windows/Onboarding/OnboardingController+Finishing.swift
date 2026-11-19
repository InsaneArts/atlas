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
  /// login has been found on screen for ``settledFor``.
  func retryFinish() {
    guard !isQueued(.check) else { return }
    snapshot.finish = .idle
    refresh()
  }

  /// Runs the end-to-end check, or a VM's, then reads the facts again; a check the closing window stopped shows nothing. Once it
  /// passed, its Continue lights up and takes presses ``continueFollows`` later.
  func check() async {
    let outcome = await commands.run(snapshot.checkedVM.map { .finishVM($0) } ?? .finishSetup)
    guard !closed else { return }
    snapshot.finish = outcome.ok ? .passed : .failed(outcome.message)
    refresh()
    guard outcome.ok else { return }
    // Cut short only as `run()` ends, which drops what follows.
    try? await dependencies.clock.sleep(for: Self.continueFollows)
    guard !closed, !Task.isCancelled, snapshot.finish == .passed else { return }
    snapshot.passSettled = true
  }

  /// Done: set Open at Login on a first setup, keep the home folder private as the box says, open Atlas, then close the window; on any
  /// failure the window stays, saying why. A home folder Atlas couldn't make private stops this Done there, with the
  /// box turned off, and the next Done finishes without it.
  func complete() async {
    guard await setOpenAtLogin(),
      await keepHomePrivate(),
      let started = await act(.openMenuBar, OnboardingWords.opening),
      started.ok
    else { return }
    closeRequested = true
  }

  /// Sets Open at Login as the switch says, on a first setup only: a desktop added from Settings leaves it as the
  /// person set it.
  ///
  /// - Returns: Whether Done goes on; false once setting it failed, which says why, or once the window has closed.
  func setOpenAtLogin() async -> Bool {
    guard !snapshot.choices.adding else { return true }
    return await act(.setOpensAtLogin(snapshot.opensAtLogin), OnboardingWords.finishing)?.ok == true
  }

  /// Makes the home folder private when the box is checked, else remembers that it was offered, unless an attempt
  /// failed: then it stays unoffered, so Atlas offers it again later. With a VM there is nothing to do.
  ///
  /// - Returns: Whether Done goes on; false after a failed attempt, which says why and turns the box off, or once the
  ///   window has closed.
  func keepHomePrivate() async -> Bool {
    // A VM has no home folder of the person's to keep private.
    guard !snapshot.onVM else { return true }
    guard snapshot.keepsHomePrivate else {
      // Through `act`, so the switches wait while it runs; a failure to remember the offer only asks again later.
      guard !snapshot.homePrivacyFailed else { return true }
      return await act(.homeOffered, OnboardingWords.finishing) != nil
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
