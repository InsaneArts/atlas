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
  /// The notice when a step failed, did not run, or may not have worked.
  static let cleanupUnfinished = "The cleanup didn't finish. Details lists each step."

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

  /// The alert's line for rolling back plans.
  ///
  /// - Parameter count: How many it rolls back.
  /// - Returns: "Rolls back 1 copy into the helper".
  static func rollsBack(_ count: Int) -> String {
    "Rolls back \(copies(count)) into the helper"
  }

  /// The alert's line for removing an installed app.
  ///
  /// - Parameter app: The app's name; nil when the service names none.
  /// - Returns: "Removes Probe from Applications".
  static func removesInstall(_ app: String?) -> String {
    "Removes \(app ?? "an app") from Applications"
  }

  /// The alert's line for an installed app the cleanup leaves, as it changed since it was installed.
  ///
  /// - Parameter app: The app's name; nil when the service names none.
  /// - Returns: "Keeps Probe, as it changed since it was installed".
  static func keepsInstall(_ app: String?) -> String {
    "Keeps \(app ?? "an app"), as it changed since it was installed"
  }

  /// The alert's line for removing copies.
  ///
  /// - Parameter count: How many it removes.
  /// - Returns: "Removes 1 copy".
  static func removesCopies(_ count: Int) -> String {
    "Removes \(copies(count))"
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
  static let rollBack = "Roll back a copy into the helper"

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

  /// What a step this app does not know did, in the receipt.
  static let otherStep = "Another step"

  /// A step that worked.
  static let stepDone = "Done"
  /// A step whose app or copy was gone before it ran.
  static let stepAlreadyGone = "Done: it was already gone"
  /// A step that did not run.
  static let stepNotRun = "Didn't run"
  /// A step that may or may not have worked.
  static let stepUnknown = "Not known whether it worked"

  /// A step left as it is on purpose.
  ///
  /// - Parameter reason: Why, as the service's word; nil for none.
  /// - Returns: "Kept: changed since it was installed", or "Kept".
  static func stepKept(_ reason: String?) -> String {
    guard let said = stepReason(reason) else { return "Kept" }
    return "Kept: \(said)"
  }

  /// A step that did not work.
  ///
  /// - Parameters:
  ///   - reason: Why, as the service's word; nil for none.
  ///   - retry: Whether cleaning up again may get it done.
  /// - Returns: "Couldn't: still in use. Try again.", "Couldn't: still in use" or "Couldn't".
  static func stepFailed(_ reason: String?, retry: Bool) -> String {
    var said = "Couldn't"
    if let why = stepReason(reason) {
      said = "Couldn't: \(why)"
    }
    return retry ? "\(said). Try again." : said
  }

  /// A step that did not run, and why.
  ///
  /// - Parameter reason: Why, as the service's word; nil for none.
  /// - Returns: "Didn't run: agents are stopped", or "Didn't run".
  static func stepBlocked(_ reason: String?) -> String {
    guard let why = stepReason(reason) else { return stepNotRun }
    return "\(stepNotRun): \(why)"
  }

  /// Why a step was kept, failed or did not run, in words; nil for a word this app does not know, which is never
  /// shown as it is.
  ///
  /// - Parameter reason: The service's word, such as `changed` or `in-use`.
  /// - Returns: "changed since it was installed", or nil.
  static func stepReason(_ reason: String?) -> String? {
    switch reason {
    case "changed": "changed since it was installed"
    case "replaced": "something else is there now"
    case "another-session": "another session made it"
    case "label-unreadable": "Atlas can't tell whose it is"
    case "kept-targets": "the helper changed some of its files since"
    case "timeout": "it didn't quit in time"
    case "in-use": "still in use"
    case "io": "the file system refused"
    case "helper": "the helper couldn't do it"
    case "needs-step": "a step before it isn't done"
    case "stopped": "agents are stopped"
    default: nil
    }
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
