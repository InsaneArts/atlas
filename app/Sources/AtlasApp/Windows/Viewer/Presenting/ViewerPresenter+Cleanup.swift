import AtlasService
import AtlasUI

extension ViewerPresenter {
  /// Clean Up This Session's Work… under the chosen session in Details, while the session left something to clean
  /// up (a copy, a plan, an installed app, or a cleanup that did not finish) or a cleanup from this window has
  /// something to say: it waits while one is read, runs or may still run, as one whose reply never came may, and lists
  /// what the last one did once it ran.
  ///
  /// - Parameters:
  ///   - record: The session, as the list has it.
  ///   - snapshot: What the controller holds.
  /// - Returns: The cleanup, or nil when there is nothing to clean up.
  static func cleanup(_ record: HostSessionRecord, _ snapshot: ViewerSnapshot) -> SessionCleanup? {
    let session = snapshot.session
    let receipt = session.cleanupReceipts[record.key]
    let unfinished = record.cleanup.map { $0 != .complete } == true
    let left = !record.copies.isEmpty || !record.plans.isEmpty || !record.installs.isEmpty || unfinished
    let running = session.cleaningUp == record.key || session.cleanupUnanswered?.session == record.key
    // Terminal's session has no key a cleanup can name.
    guard !record.key.isEmpty, left || receipt != nil || running else { return nil }
    let stage: SessionCleanup.Stage =
      if session.cleaningUp != nil || session.cleanupUnanswered != nil {
        .working
      } else if let receipt {
        .finished(cleanupReceipt(receipt))
      } else {
        .ready
      }
    return SessionCleanup(session: record.key, title: ViewerWords.cleanUpSession, stage: stage)
  }

  /// The alert's lines, from what a preview says it takes: the apps it quits as one line, the environment changes it
  /// undoes and the copies it removes counted, and each installed app it removes, or keeps as it changed. A step a cleanup
  /// that did not finish before already did, or kept, is left out.
  ///
  /// - Parameter preview: What cleaning up takes.
  /// - Returns: The lines, in the order the steps run; none when there are no steps.
  static func cleanupSteps(_ preview: CleanupReply) -> [String] {
    let open = preview.steps.filter { $0.state != .done && $0.state != .kept }
    guard !open.isEmpty else { return preview.isEmpty ? [] : [ViewerWords.finishLastCleanup] }
    let kinds = { (kind: CleanupStepKind) in open.filter { $0.kind == kind } }
    var lines: [String] = []
    let quits = kinds(.quit)
    if !quits.isEmpty {
      var apps: [String] = []
      for app in quits.flatMap(\.apps) where !apps.contains(app) { apps.append(app) }
      lines.append(ViewerWords.quits(apps))
    }
    let plans = kinds(.undoPlan).count
    if plans > 0 { lines.append(ViewerWords.undoesChanges(plans)) }
    for install in kinds(.removeInstall) {
      lines.append(
        install.unchanged == false ? ViewerWords.keepsInstall(install.app) : ViewerWords.removesInstall(install.app)
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
  /// purpose reads apart from one that failed.
  ///
  /// - Parameter reply: The apply's reply.
  /// - Returns: One fact a step, in order.
  static func cleanupReceipt(_ reply: CleanupReply) -> [Fact] {
    reply.steps.map { step in
      Fact(name: cleanupStepName(step), value: cleanupOutcome(step))
    }
  }

  /// What a step did, in the receipt.
  ///
  /// - Parameter step: The step.
  /// - Returns: "Remove Probe from Applications".
  static func cleanupStepName(_ step: CleanupStep) -> String {
    switch step.kind {
    case .quit: ViewerWords.quit(step.apps)
    case .undoPlan: ViewerWords.undoChange
    case .removeInstall: ViewerWords.removeInstall(step.app)
    case .removeCopy: ViewerWords.removeCopy(step.app)
    case .unknown: ViewerWords.otherStep
    }
  }

  /// How a step went, in the receipt.
  ///
  /// - Parameter step: The step.
  /// - Returns: "Done", "Kept: changed since it was installed" or "Couldn't: still in use. Try again.".
  static func cleanupOutcome(_ step: CleanupStep) -> String {
    switch step.state {
    case .done: step.alreadyGone ? ViewerWords.stepAlreadyGone : ViewerWords.stepDone
    case .kept: ViewerWords.stepKept(step.reason)
    case .failed: ViewerWords.stepFailed(step.reason, retry: step.retry)
    case .blocked: ViewerWords.stepBlocked(step.reason)
    case .pending, nil: ViewerWords.stepNotRun
    case .started, .uncertain, .unknown: ViewerWords.stepUnknown
    }
  }

  /// The notice once a cleanup ran: cleaned up, nothing left to clean up, or that it did not finish: not knowing
  /// whether its last step finished and stopping with agents stopped each read on their own.
  ///
  /// - Parameter reply: The apply's reply.
  /// - Returns: The notice.
  static func cleanupNotice(_ reply: CleanupReply) -> String {
    switch reply.state {
    case .uncertain: return ViewerWords.cleanupUncertain
    case .blocked: return ViewerWords.cleanupBlocked
    case .complete, .incomplete, .unknown, nil: break
    }
    guard reply.ok, reply.state == nil || reply.state == .complete else { return ViewerWords.cleanupUnfinished }
    return reply.isEmpty ? ViewerWords.nothingToCleanUp : ViewerWords.cleanedUp
  }

  /// The notice once a cleanup whose reply never came ended, from what `cleanup.json` lists then: steps a run left
  /// read as an apply's do, and with nothing left it cleaned up.
  ///
  /// - Parameter preview: What cleaning up takes now.
  /// - Returns: The notice; nil when no step has run, as nothing is known of how it went.
  static func unansweredCleanupNotice(_ preview: CleanupReply) -> String? {
    guard !preview.isEmpty else { return ViewerWords.cleanedUp }
    let states = preview.steps.compactMap(\.state)
    guard !states.isEmpty else { return nil }
    if states.contains(where: { $0 == .started || $0 == .uncertain }) { return ViewerWords.cleanupUncertain }
    return states.contains(.blocked) ? ViewerWords.cleanupBlocked : ViewerWords.cleanupUnfinished
  }
}
