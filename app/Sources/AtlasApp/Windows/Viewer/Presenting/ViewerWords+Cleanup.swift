import AtlasService

/// Cleaning up a session's work: the button, the alert that asks first with a line for each thing it does, what each
/// step did once it ran, and the notice after. Every word comes from a step's kind, app names, state and reason, never
/// from a path.
extension ViewerWords {
  /// The alert's title.
  static let cleanupTitle = "Clean up this session's work?"
  /// The button last in the session's part of Details.
  static let cleanUpSession = "Clean Up This Session's Work…"
  /// The alert's red button.
  static let cleanupButton = "Clean Up"
  /// What VoiceOver hears for Clean Up while it runs.
  static let cleaningUp = "Cleaning Up…"
  /// The alert's warning while the session acted in the last few minutes, or its host still runs.
  static let mayStillBeWorking = "This session may still be working."
  /// The alert's one line when every step is left from a cleanup that did not finish before.
  static let finishLastCleanup = "Finishes the last cleanup"
  /// The notice when the session left nothing to clean up after all.
  static let nothingToCleanUp = "Nothing left to clean up for this session."
  /// The notice once every step is done, or kept on purpose.
  static let cleanedUp = "Cleaned up this session's work."
  /// The notice when a step failed or did not run.
  static let cleanupUnfinished = "The cleanup didn't finish. Details lists each step."
  /// The notice when Atlas does not know whether a step worked, which Recover settles before cleaning up again.
  static let cleanupUncertain =
    "Atlas doesn't know whether the last step finished. Recover the helper first, then clean up again."
  /// The notice when the person stopped agents between steps, which stopped the cleanup too.
  static let cleanupBlocked = "Agents are stopped, so the cleanup stopped. Resume, then clean up again."
  /// The notice when a collected copy is missing or changed, so the cleanup kept the files it came from.
  static let cleanupOutputsNotKept =
    "A file collected from the helper is no longer kept here, so the cleanup left what it came from. Collect it again, then clean up again."
  /// The notice when the helper could not be reached to stop a background job, which held the rest back.
  static let cleanupJobsUnreached =
    "Atlas couldn't reach the helper to stop a background job. Recover the helper, then clean up again."
  /// The notice when the cleanup's reply never came: it may still run, and the button waits until it ended.
  static let cleanupUnanswered = "The cleanup is taking a while. Details shows each step once it ends."

  /// The alert's sentence: a line for each thing it does, then the warning after a blank line.
  ///
  /// - Parameters:
  ///   - steps: What it does, a line each.
  ///   - warning: That the session may still be working; nil for none.
  /// - Returns: "Quits Probe\nRemoves 1 copy\n\nThis session may still be working."
  static func cleanupMessage(_ steps: [String], warning: String?) -> String {
    let lines = steps.joined(separator: "\n")
    guard let warning else { return lines }
    return lines.isEmpty ? warning : "\(lines)\n\n\(warning)"
  }

  /// The alert's line for quitting apps.
  ///
  /// - Parameter apps: The apps it quits, by name; none when the service names none.
  /// - Returns: "Quits Probe and Notes", or "Quits what runs from its copies".
  static func quits(_ apps: [String]) -> String {
    apps.isEmpty ? "Quits what runs from its copies" : "Quits \(listed(apps))"
  }

  /// The alert's line for undoing plans, each a change to the helper's environment.
  ///
  /// - Parameter count: How many it undoes.
  /// - Returns: "Undoes 1 environment change" or "Undoes 2 environment changes".
  static func undoesChanges(_ count: Int) -> String {
    count == 1 ? "Undoes 1 environment change" : "Undoes \(count) environment changes"
  }

  /// The alert's line for moving an installed app aside in the helper, which removes it after 7 days.
  ///
  /// - Parameter app: The app's name; nil when the service names none.
  /// - Returns: "Moves Probe aside in the helper; removed after 7 days".
  static func holdsInstall(_ app: String?) -> String {
    "Moves \(app ?? "an app") aside in the helper; removed after 7 days"
  }

  /// The alert's line for an installed app the cleanup leaves, as it changed since it was installed.
  ///
  /// - Parameter app: The app's name; nil when the service names none.
  /// - Returns: "Keeps Probe, as it changed since it was installed".
  static func keepsInstall(_ app: String?) -> String {
    "Keeps \(app ?? "an app"), as it changed since it was installed"
  }

  /// The alert's line for removing the copies Atlas made of the person's files.
  ///
  /// - Parameter count: How many it removes.
  /// - Returns: "Removes 1 copy".
  static func removesCopies(_ count: Int) -> String {
    "Removes \(copies(count))"
  }

  /// The alert's line for checking the files the session collected out of the helper.
  static let checksOutputs = "Checks the files it collected are still kept"

  /// The alert's line for stopping background jobs: the job by name when it stops one the service names, else how
  /// many it stops, so a job with no name still counts.
  ///
  /// - Parameter names: Each job's name, in order; nil for a job the service names none for.
  /// - Returns: "Stops the background job web", "Stops 1 background job" or "Stops 2 background jobs".
  static func stopsJobs(_ names: [String?]) -> String {
    if names.count == 1, let name = names[0] { return "Stops \(job(name))" }
    return names.count == 1 ? "Stops 1 background job" : "Stops \(names.count) background jobs"
  }

  /// The alert's line for steps this app does not know.
  ///
  /// - Parameter count: How many there are.
  /// - Returns: "Takes 1 more step".
  static func moreSteps(_ count: Int) -> String {
    count == 1 ? "Takes 1 more step" : "Takes \(count) more steps"
  }

  /// What a quit step did, in the receipt.
  ///
  /// - Parameter apps: The apps it quit, by name; none when the service names none.
  /// - Returns: "Quit Probe", or "Quit what runs from its copies".
  static func quit(_ apps: [String]) -> String {
    apps.isEmpty ? "Quit what runs from its copies" : "Quit \(listed(apps))"
  }

  /// What a plan's step did, in the receipt.
  static let undoChange = "Undo an environment change"

  /// What an install's step did, in the receipt.
  ///
  /// - Parameter app: The app's name; nil when the service names none.
  /// - Returns: "Remove Probe from Applications".
  static func removeInstall(_ app: String?) -> String {
    "Remove \(app ?? "an app") from Applications"
  }

  /// What a copy's step did, in the receipt.
  ///
  /// - Parameter app: The copied app's name; nil when the service names none.
  /// - Returns: "Remove the copy of Probe", or "Remove a copy".
  static func removeCopy(_ app: String?) -> String {
    app.map { "Remove the copy of \($0)" } ?? "Remove a copy"
  }

  /// What a check of the collected files did, in the receipt.
  static let checkOutputs = "Check the collected files"

  /// What a job's step did, in the receipt.
  ///
  /// - Parameter name: The job's name; nil when the service names none.
  /// - Returns: "Stop the background job web", or "Stop a background job".
  static func stopJob(_ name: String?) -> String {
    "Stop \(job(name))"
  }

  /// The receipt's name for the processes that left a stopped job and still run.
  static let stillRunning = "Still running"
  /// A process that left a stopped job whose name the service does not give.
  static let unnamedProcess = "a process"

  /// The processes that left a stopped job and still run, which Atlas did not stop.
  ///
  /// - Parameter names: Their command names, in order.
  /// - Returns: "sleep and node; Atlas didn't stop them".
  static func leftRunning(_ names: [String]) -> String {
    "\(listed(names)); Atlas didn't stop \(names.count == 1 ? "it" : "them")"
  }

  /// The receipt's name for the files whose collected copies are missing or changed.
  static let notKept = "Not kept here"

  /// The files whose collected copies are missing or changed, which the helper keeps.
  ///
  /// - Parameter count: How many.
  /// - Returns: "1 collected file; the helper keeps it", or "2 collected files; the helper keeps them".
  static func notKeptFiles(_ count: Int) -> String {
    count == 1 ? "1 collected file; the helper keeps it" : "\(count) collected files; the helper keeps them"
  }

  /// What a step this app does not know did, in the receipt.
  static let otherStep = "Another step"

  /// A step that worked.
  static let stepDone = "Done"
  /// An installed app's step that moved it aside in the helper, which removes it after 7 days.
  static let stepHeld = "Moved aside in the helper; removed after 7 days"
  /// A step whose app or copy was gone before it ran.
  static let stepAlreadyGone = "Done: it was already gone"
  /// A step that did not run.
  static let stepNotRun = "Didn't run"
  /// A step that may or may not have worked.
  static let stepUnknown = "Not known whether it worked"

  /// A step left as it is on purpose.
  ///
  /// - Parameter reason: Why; nil for none.
  /// - Returns: "Kept: changed since it was installed", or "Kept".
  static func stepKept(_ reason: CleanupStep.Reason?) -> String {
    guard let said = stepReason(reason) else { return "Kept" }
    return "Kept: \(said)"
  }

  /// A step that did not work.
  ///
  /// - Parameters:
  ///   - reason: Why; nil for none.
  ///   - retry: Whether cleaning up again may get it done.
  /// - Returns: "Couldn't: still in use. Try again.", "Couldn't: still in use" or "Couldn't".
  static func stepFailed(_ reason: CleanupStep.Reason?, retry: Bool) -> String {
    var said = "Couldn't"
    if let why = stepReason(reason) {
      said = "Couldn't: \(why)"
    }
    return retry ? "\(said). Try again." : said
  }

  /// A step that did not run, and why.
  ///
  /// - Parameter reason: Why; nil for none.
  /// - Returns: "Didn't run: agents are stopped", or "Didn't run".
  static func stepBlocked(_ reason: CleanupStep.Reason?) -> String {
    guard let why = stepReason(reason) else { return stepNotRun }
    return "\(stepNotRun): \(why)"
  }

  /// Why a step was kept, failed or did not run, in words; nil for none, for a reason this app does not know, which is
  /// never shown as it is, and for an unknown outcome, which the step's own value already says.
  ///
  /// - Parameter reason: Why, such as ``CleanupStep/Reason/changed``.
  /// - Returns: "changed since it was installed", or nil.
  static func stepReason(_ reason: CleanupStep.Reason?) -> String? {
    switch reason {
    case .changed: "changed since it was installed"
    case .replaced: "something else is there now"
    case .anotherSession: "another session made it"
    case .labelUnreadable: "Atlas can't tell whose it is"
    case .keptTargets: "the helper changed some of its files since"
    case .timeout: "it didn't quit in time"
    case .inUse: "still in use"
    case .io: "the file system refused"
    case .helper: "the helper couldn't do it"
    case .needsStep: "a step before it isn't done"
    case .stopped: "agents are stopped"
    case .otherVolume: "it could not be moved aside without copying"
    case .unreadable: "Atlas couldn't read its record of it"
    case .outputNotKept: "a collected copy is missing or changed"
    case .stillStopping: "the job is still stopping"
    case .identity: "the job is no longer the process Atlas started"
    case .otherAccount: "part of the job runs as another account"
    case .daemon: "Atlas couldn't reach the helper"
    case .leaderGone: "the job's first process is gone"
    case .uncertain, .unknown, nil: nil
    }
  }

  /// A background job, by its name when the service gives one, as the alert and the receipt both name it.
  ///
  /// - Parameter name: The job's name; nil when the service names none.
  /// - Returns: "the background job web", or "a background job".
  private static func job(_ name: String?) -> String {
    name.map { "the background job \($0)" } ?? "a background job"
  }

  /// A count of copies.
  ///
  /// - Parameter count: How many.
  /// - Returns: "1 copy" or "2 copies".
  private static func copies(_ count: Int) -> String {
    count == 1 ? "1 copy" : "\(count) copies"
  }

  /// Names as one phrase.
  ///
  /// - Parameter names: The names, in order.
  /// - Returns: "Probe", "Probe and Notes" or "Probe, Notes and Mail".
  private static func listed(_ names: [String]) -> String {
    guard let last = names.last, names.count > 1 else { return names.first ?? "" }
    return "\(names.dropLast().joined(separator: ", ")) and \(last)"
  }
}
