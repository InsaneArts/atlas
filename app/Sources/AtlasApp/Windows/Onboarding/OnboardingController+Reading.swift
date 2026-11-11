import AtlasCommands
import AtlasOnboarding
import AtlasService
import AtlasUI

/// The reading loop: the facts read now, then as ``ReadingPace`` says, one reading at a time, beside the queue, so a
/// reading never waits behind a command.
extension OnboardingController {
  /// Reads the facts when the pace says, until the window closes; a refresh cuts the pause short, but never brings a
  /// reading closer than the pace allows. While the person is away, nothing is read until they are back.
  func readFacts() async {
    while !closed, !Task.isCancelled {
      // Every reading, a refresh's included, waits while the person is away.
      guard !isAway else {
        await untilBack()
        continue
      }
      let wait = pace.next - opened()
      guard wait <= .zero else {
        await rest(for: wait)
        continue
      }
      pace.start(at: opened())
      await read()
    }
  }

  /// Notes each change the window's watcher reports, until the window closes: the reading comes as the pace allows.
  func followChanges() async {
    for await _ in changes.changes {
      guard !closed else { return }
      pace.change(at: opened())
      wake()
    }
  }

  /// Whether the person is away: neither this login nor the helper has the Mac's screen, as while the login window has
  /// it during a switch, read now in this process. Reading then helps nothing, and each reading's processes would land
  /// on the switch; while the helper has the screen the readings carry on, since they drive its setup once Atlas is
  /// allowed there. Until the first reading names the helper, nobody is away.
  private var isAway: Bool {
    snapshot.loaded && snapshot.facts.role == .controller && !dependencies.console.refresh()
      && dependencies.consoleUser() != snapshot.facts.helper.name
  }

  /// Waits, reading only who has the screen, until the person is back, then ``quietAfterSwitch`` more for the switch to
  /// settle, after which the facts are read at once. A change (the screen changing hands) cuts the wait short; who has
  /// the screen is asked again every ``ReadingPace/minimumGap`` all the same, which starts no process: macOS reports
  /// no change for an unlock, and may report the switch before this login reads as on screen. The time this login has
  /// been on screen counts again from none.
  private func untilBack() async {
    leftScreen()
    while isAway, !closed, !Task.isCancelled { await rest(for: ReadingPace.minimumGap) }
    guard !closed, !Task.isCancelled else { return }
    settling = true
    await rest(for: Self.quietAfterSwitch)
    settling = false
    pace.ask(at: opened())
  }

  /// Reads the facts again soon: now, or right after the reading under way; once back from being away, only after
  /// ``quietAfterSwitch``, which a refresh never cuts short.
  func refresh() {
    guard !closed else { return }
    pace.ask(at: opened())
    wake()
  }

  /// Watches closely from now, as after a change: the person is about to do something macOS may not report, such as
  /// turning Screen Recording on in System Settings.
  func watchClosely() {
    guard !closed else { return }
    pace.change(at: opened())
    wake()
  }

  /// Cuts the loop's pause short so it works out its next reading again, unless it is settling after a switch. This
  /// login off the screen now ends the time it has been found there.
  private func wake() {
    if !dependencies.console.refresh() { leftScreen() }
    if !settling { resting?.cancel() }
  }

  /// This login is not on screen: the time it has been found there counts again from none.
  private func leftScreen() {
    onConsoleSince = nil
    pace.drop(.settled)
  }

  /// Whether this login has been on screen ``settledFor`` by the readings, and still is.
  private var settledOnScreen: Bool {
    guard let onConsoleSince else { return false }
    let console = dependencies.console
    return opened() - onConsoleSince >= Self.settledFor && console.refresh() && console.departures == departuresThen
  }

  /// A pause of `duration` on the clock, until it ends, a refresh or the close cuts it short, or the loop is cancelled.
  private func rest(for duration: Duration) async {
    // Unstructured, so a refresh or the window's close can cut the pause short; the loop waits for it.
    let pause = Task<Void, Never> { [clock = dependencies.clock] in try? await clock.sleep(for: duration) }
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
      noteScreen()
      // A job restarting (to pick up Screen Recording) leaves its permissions unknown, never off.
      facts.asker = facts.asker.keepingLastKnown(of: snapshot.facts.asker, restarting: restarting(facts))
      snapshot.facts = facts
      changes.watch(folders: facts.watchedFolders(home: dependencies.home))
      for permission in Permission.allCases where permission.granted(to: facts.asker) {
        snapshot.allowedBefore.insert(permission)
      }
      if facts.role == .helper, facts.asker.name != askerPictured {
        askerPictured = facts.asker.name
        snapshot.askerPicture = dependencies.accountPicture(facts.asker.name)
      }
      if !snapshot.loaded {
        snapshot.opensAtLogin = facts.opensAtLogin(enabledNow: dependencies.loginItem.isEnabled())
        snapshot.keepsHomePrivate = facts.keepsHomePrivate
        // A failed first reading's message goes with it.
        snapshot.problem = nil
      }
      snapshot.loaded = true
      // A pick the choice no longer shows, or none yet, gives way to the default.
      if snapshot.shownPick == nil { snapshot.picked = facts.defaultPick.map(OnboardingAccountPick.init(name:)) }
      timeFirstLogin()
      await advance()
    } else {
      // A reading that told nothing proves nothing about this login since the last one.
      leftScreen()
      if !snapshot.loaded { snapshot.problem = Self.firstReadingProblem(outcome) }
    }
  }

  /// Notes whether this reading found this login on screen, and since when: a stay broken since, however briefly,
  /// begins again now.
  private func noteScreen() {
    let console = dependencies.console
    guard console.refresh() else { return leftScreen() }
    guard onConsoleSince == nil || console.departures != departuresThen else { return }
    onConsoleSince = opened()
    departuresThen = console.departures
  }

  /// Starts the check once this login has been on screen ``settledFor``; until then, asks for the reading that finds
  /// it so.
  private func startWhenSettled() {
    guard !settledOnScreen else { return runFinish() }
    if let onConsoleSince { pace.read(.settled, at: onConsoleSince + Self.settledFor) }
  }

  /// Whether a job whose permissions `facts` finds unknown may still be restarting: for ``restartGrace`` from the first
  /// reading that found them so, with a reading asked for as the grace ends, which finds it stopped. Only the helper
  /// reads its job's permissions, so in the person's account nothing is timed.
  private func restarting(_ facts: OnboardingFacts) -> Bool {
    guard facts.role == .helper, !facts.asker.permissionsKnown else {
      unknownSince = nil
      pace.drop(.restartGrace)
      return true
    }
    let now = opened()
    if unknownSince == nil {
      unknownSince = now
      pace.read(.restartGrace, at: now + Self.restartGrace)
    }
    return now - (unknownSince ?? now) < Self.restartGrace
  }

  /// Why the first reading failed, in words: a reply this window does not understand; Atlas's command line that
  /// could not run at all, answering nothing, said as a plain cause and what to do; else the command's own reason.
  ///
  /// - Parameter outcome: The first reading's outcome.
  /// - Returns: The problem.
  static func firstReadingProblem(_ outcome: Outcome) -> String {
    if outcome.ok { return OnboardingWords.notUnderstood }
    if outcome.reply.isEmpty { return OnboardingWords.filesIncomplete }
    return Words.sentence(outcome.message)
  }

  /// Starts what a step needs as soon as it is shown, moves the permission panel on once its permission is on, and
  /// marks the helper's window open.
  private func advance() async {
    guard !closed else { return }
    let step = snapshot.step
    switch step {
    case .finishing where snapshot.finish == .idle: startWhenSettled()
    case .agents where snapshot.agents.isEmpty: loadAgents()
    case .agents where snapshot.agentsFailed: retryAgents()
    default: break
    }
    // Off the agents step, nothing calls for asking them again; off the check, nothing waits to be settled.
    if step != .agents { pace.drop(.agentsRetry) }
    if step != .finishing { pace.drop(.settled) }
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
    // While the panel is open its switch may turn on at any moment, and macOS says nothing for Screen Recording. A
    // panel left open is no longer watched closely after ``panelWatch``.
    if snapshot.guiding != nil, let panelOpenedAt, opened() - panelOpenedAt < Self.panelWatch {
      pace.change(at: opened())
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
