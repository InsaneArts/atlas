import AtlasService
import AtlasUI

extension ViewerPresenter {
  /// Clean Up This Session's Work… in a session's menu and the Session menu, while the session left something to clean
  /// up (a copy, a plan, an installed app, or a cleanup that did not finish) or a cleanup from this window has
  /// something to say: it waits while one is read, runs or may still run, as one whose reply never came may, and keeps
  /// what the last one did once it ran.
  ///
  /// - Parameters:
  ///   - record: The session, as the list has it.
  ///   - snapshot: What the controller holds.
  /// - Returns: The cleanup, or nil when there is nothing to clean up.
  static func cleanup(_ record: HostSessionRecord, _ snapshot: ViewerSnapshot) -> ViewerSessionCleanup? {
    let state = snapshot.session.cleanup
    let receipt = state.receipts[record.key]
    let running = state.isBusy(for: record.key)
    // Terminal's session has no key a cleanup can name.
    guard !record.key.isEmpty, record.leftWork || receipt != nil || running else { return nil }
    let stage: ViewerSessionCleanup.Stage =
      if running {
        .working
      } else if let receipt {
        .finished(cleanupReceipt(receipt))
      } else {
        .ready
      }
    return ViewerSessionCleanup(
      session: record.key,
      stage: stage,
      // One cleanup runs at a time: another session's holds this one, its row with no spinner of its own.
      waitsForAnother: state.isBusy && !running
    )
  }

  /// Cleaning up the chosen session's work, as the Session menu offers it: the chosen session's cleanup while the
  /// sessions' list names it; none while every session's work shows.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The cleanup, or nil.
  static func chosenCleanup(_ snapshot: ViewerSnapshot) -> ViewerSessionCleanup? {
    guard case .one(let key) = snapshot.shownSessions, let record = snapshot.panes.sessions.record(key) else {
      return nil
    }
    return cleanup(record, snapshot)
  }

  /// The alert's lines, from what a preview says it takes: the apps it quits as one line, the environment changes it
  /// undoes and the copies it removes counted, and each installed app it moves aside in the helper, or keeps as it
  /// changed. A step a cleanup that did not finish before already did, or kept, is left out.
  ///
  /// - Parameter preview: What cleaning up takes.
  /// - Returns: The lines, in the order the steps run; none when there are no steps.
  static func cleanupSteps(_ preview: CleanupReply) -> [String] {
    let open = preview.openSteps
    guard !open.isEmpty else { return preview.isEmpty ? [] : [ViewerWords.finishLastCleanup] }
    let kinds = { (kind: CleanupStepKind) in open.filter { $0.kind == kind } }
    var lines: [String] = []
    if !kinds(.checkOutputs).isEmpty { lines.append(ViewerWords.checksOutputs) }
    let quits = kinds(.quit)
    if !quits.isEmpty {
      var apps: [String] = []
      for app in quits.flatMap(\.apps) where !apps.contains(app) { apps.append(app) }
      lines.append(ViewerWords.quits(apps))
    }
    let jobs = kinds(.stopJob)
    if !jobs.isEmpty { lines.append(ViewerWords.stopsJobs(jobs.map(\.name))) }
    let plans = kinds(.undoPlan).count
    if plans > 0 { lines.append(ViewerWords.undoesChanges(plans)) }
    for install in kinds(.removeInstall) {
      lines.append(
        install.unchanged == false ? ViewerWords.keepsInstall(install.app) : ViewerWords.holdsInstall(install.app)
      )
    }
    let copies = kinds(.removeCopy).count
    if copies > 0 { lines.append(ViewerWords.removesCopies(copies)) }
    let others = kinds(.unknown).count
    if others > 0 { lines.append(ViewerWords.moreSteps(others)) }
    return lines
  }

  /// The alert's warning: that the session may still be working, while it acted lately or its host still runs.
  ///
  /// - Parameter preview: What cleaning up takes.
  /// - Returns: The warning, or nil.
  static func cleanupWarning(_ preview: CleanupReply) -> String? {
    preview.active ? ViewerWords.mayStillBeWorking : nil
  }

  /// What each step of a cleanup that ran did: its name what the step did, its value how it went, so a step kept on
  /// purpose reads apart from one that failed; after a job's step, what left the job and still runs, and after a
  /// check of the collected files, how many of them are no longer kept here.
  ///
  /// - Parameter reply: The apply's reply.
  /// - Returns: A fact a step, and one more for each of those, in order.
  static func cleanupReceipt(_ reply: CleanupReply) -> [Fact] {
    reply.steps.flatMap { step in
      var facts = [Fact(name: cleanupStepName(step), value: cleanupOutcome(step))]
      if !step.left.isEmpty {
        let names = step.left.map { $0.name.isEmpty ? ViewerWords.unnamedProcess : $0.name }
        facts.append(Fact(name: ViewerWords.stillRunning, value: ViewerWords.leftRunning(names)))
      }
      if step.notKept > 0 {
        facts.append(Fact(name: ViewerWords.notKept, value: ViewerWords.notKeptFiles(step.notKept)))
      }
      return facts
    }
  }

  /// What a step did, in the receipt.
  ///
  /// - Parameter step: The step.
  /// - Returns: "Remove Probe from Applications".
  static func cleanupStepName(_ step: CleanupStep) -> String {
    switch step.kind {
    case .checkOutputs: ViewerWords.checkOutputs
    case .quit: ViewerWords.quit(step.apps)
    case .stopJob: ViewerWords.stopJob(step.name)
    case .undoPlan: ViewerWords.undoChange
    case .removeInstall: ViewerWords.removeInstall(step.app)
    case .removeCopy: ViewerWords.removeCopy(step.app)
    case .unknown: ViewerWords.otherStep
    }
  }

  /// How a step went, in the receipt.
  ///
  /// - Parameter step: The step.
  /// - Returns: "Done", "Moved aside in the helper; removed after 7 days", "Kept: changed since it was installed" or
  ///   "Couldn't: still in use. Try again.".
  static func cleanupOutcome(_ step: CleanupStep) -> String {
    switch step.state {
    case .done where step.alreadyGone: ViewerWords.stepAlreadyGone
    case .done where step.held != nil && step.kind == .removeInstall:
      ViewerWords.stepHeld
    case .done: ViewerWords.stepDone
    case .kept: ViewerWords.stepKept(step.reason)
    case .failed: ViewerWords.stepFailed(step.reason, retry: step.retry)
    case .blocked: ViewerWords.stepBlocked(step.reason)
    case .pending, nil: ViewerWords.stepNotRun
    case .started, .uncertain, .unknown: ViewerWords.stepUnknown
    }
  }

  /// The notice once a cleanup ran, by how it ended (``CleanupReply/ending``).
  ///
  /// - Parameter reply: The apply's reply.
  /// - Returns: The notice.
  static func cleanupNotice(_ reply: CleanupReply) -> String {
    endingNotice(reply.ending)
  }

  /// The notice once a cleanup whose reply never came ended, by how what `cleanup.json` lists then says it ended
  /// (``CleanupReply/unansweredEnding``).
  ///
  /// - Parameter preview: What cleaning up takes now.
  /// - Returns: The notice; nil when no step has run, as nothing is known of how it went.
  static func unansweredCleanupNotice(_ preview: CleanupReply) -> String? {
    preview.unansweredEnding
      .map(endingNotice)
  }

  /// A cleanup's ending in words: cleaned up, nothing left, that it did not finish, that whether its last step
  /// finished is not known, or why it was held back: a collected copy that is no longer kept, a helper that could not
  /// be reached to stop a job, or agents stopped.
  ///
  /// - Parameter ending: How it ended.
  /// - Returns: The notice.
  static func endingNotice(_ ending: CleanupEnding) -> String {
    switch ending {
    case .cleanedUp: ViewerWords.cleanedUp
    case .nothingToCleanUp: ViewerWords.nothingToCleanUp
    case .unfinished: ViewerWords.cleanupUnfinished
    case .uncertain: ViewerWords.cleanupUncertain
    case .blocked(.outputNotKept): ViewerWords.cleanupOutputsNotKept
    case .blocked(.daemon): ViewerWords.cleanupJobsUnreached
    case .blocked: ViewerWords.cleanupBlocked
    }
  }
}
