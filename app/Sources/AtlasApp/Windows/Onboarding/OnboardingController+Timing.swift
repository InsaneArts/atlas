import AtlasCommands

/// The helper's first login after Lock and Switch: when Atlas there first answers and first hides, noted in the login
/// timing, and, while Atlas there stays silent for long, a line saying the login is slow. Atlas never stops a login.
extension OnboardingController {
  /// How long a login may run with Atlas silent before the switch screen says it is slow: 3 minutes.
  static let slowAfter: Duration = .seconds(180)

  /// Lock and Switch: the screen locks for the helper, and the login's timing starts on the clock. A screen that did
  /// not lock says so in plain words, with the keys that lock it by hand; a switch Atlas refused keeps its reason.
  func lockAndSwitch() async {
    let helper = snapshot.facts.helper
    guard let outcome = await act(.switchTo(helper.name), Words.locking) else { return }
    guard outcome.ok else {
      if outcome.code == Self.lockFailed { snapshot.problem = OnboardingWords.lockFailed(helper.title) }
      return
    }
    sinceSwitch = Self.stopwatch(dependencies.clock)
    timed = []
  }

  /// The code `atlas switch` fails with when the screen could not be locked from here, as against `not-ready`, a
  /// switch Atlas refused while a login could hang.
  static let lockFailed = "switch"

  /// After a switch, on each reading: notes when Atlas in the helper first answers and first hides, and whether the
  /// login has run past ``slowAfter`` with Atlas silent. Timing ends once both are noted.
  func timeFirstLogin() {
    guard let since = sinceSwitch else { return }
    let helper = snapshot.facts.helper
    let elapsed = since()
    for (step, reached) in [("answering", helper.answering), ("hidden", helper.hidden)]
    where reached && !timed.contains(step) {
      timed.insert(step)
      dependencies.mark("helper.\(step) after=\(LoginLog.seconds(elapsed))")
    }
    snapshot.slowFirstLogin = !helper.answering && elapsed > Self.slowAfter
    if timed.count == 2 { sinceSwitch = nil }
  }

  /// How long since now, on `clock`.
  ///
  /// - Parameter clock: The clock.
  /// - Returns: The time since the call, read on `clock` each time it is asked.
  static func stopwatch<C: Clock<Duration>>(_ clock: C) -> @Sendable () -> Duration {
    let start = clock.now
    return { start.duration(to: clock.now) }
  }
}
