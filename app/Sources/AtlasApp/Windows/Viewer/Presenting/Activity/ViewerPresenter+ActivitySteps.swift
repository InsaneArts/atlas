import AtlasService
import AtlasStatus
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// One step of a run: its kind and mark, its words without the run's app, the minute it was done, the screenshot it
  /// took framed in place, whether it kept a picture right before its first action, and for a step that did not work
  /// why and what the same stream did next. The action whose outcome is
  /// unknown is marked as a problem, says that every session waits, and offers Show Screen and Recover…. Typing into a
  /// password field says its text was not kept.
  ///
  /// - Parameters:
  ///   - run: The step's rows.
  ///   - moment: Its app run.
  ///   - moments: Every run in scope, for what followed a problem.
  ///   - snapshot: What the controller holds.
  /// - Returns: The step.
  static func activityStep(
    _ run: ActivityRun,
    in moment: ActivityMoment,
    moments: ActivityMoments,
    _ snapshot: ViewerSnapshot
  ) -> ViewerActivityStep {
    let entry = run.entry
    let offersRecovery = run.id == snapshot.reading.statusReading.uncertainRow
    let mark = stepMark(run, snapshot)
    var problem: ViewerActivityProblem?
    if offersRecovery {
      problem = ViewerActivityProblem(why: ViewerWords.unfinishedReason)
    } else if [.failed, .notSent].contains(entry.outcome) {
      problem = ViewerActivityProblem(
        why: entry.reason.map(Words.sentence) ?? "",
        next: moments.followUp(to: run)
          .map { followUpWords($0, after: run, in: moment) } ?? ""
      )
    }
    return ViewerActivityStep(
      id: run.id,
      kind: offersRecovery ? .problem : stepKind(entry),
      mark: mark,
      words: stepWords(run, in: moment),
      time: run.startedAt.map { minute($0, snapshot) } ?? "",
      inPlace: pictureTile(run.shot, snapshot),
      hasPicture: run.firstBefore != nil,
      problem: problem,
      offersRecovery: offersRecovery,
      notKept: entry.facts?.action == .type && entry.facts?.secure == true
    )
  }

  /// How a step went, for its disc's colour: the action of unknown outcome as a problem.
  ///
  /// - Parameters:
  ///   - run: The step's rows.
  ///   - snapshot: What the controller holds.
  /// - Returns: The mark.
  static func stepMark(_ run: ActivityRun, _ snapshot: ViewerSnapshot) -> ViewerStepMark {
    switch (run.id == snapshot.reading.statusReading.uncertainRow, run.entry.outcome) {
    case (true, _), (false, .failed): .failed
    case (false, .running): .live
    case (false, .notSent): .notSent
    case (false, .worked): .quiet
    }
  }

  /// What kind of thing a step did, for its disc's glyph: a problem whatever it tried, the person's own as theirs
  /// unless it handed control over, and anyone else's by what it did.
  ///
  /// - Parameter entry: The step's row.
  /// - Returns: The kind.
  static func stepKind(_ entry: ActivityLog.Entry) -> ViewerActivityKind {
    let action = ActivityAction(entry)
    if entry.outcome == .failed { return .problem }
    if entry.actor == .you, action != .control { return .person }
    switch action {
    case .pointer: return .pointer
    case .typing: return .typing
    case .keys: return .keys
    case .scroll: return .scroll
    case .screenshot: return .screenshot
    case .window: return .window
    case .control: return .control
    case .command: return .command
    }
  }

  /// What a step did, in the service's own words less the run's app and, in the person's own run, less "You",
  /// which its row already names: the control or field it named set apart in semibold, the keys it pressed as caps
  /// in place of their label, followed by the element with focus where Atlas named it, and the repeats counted. A
  /// command says it ran or failed and never what it ran. Typing shows what was typed, shortened to one line, only
  /// where Atlas kept it beside its picture; typing into a password field says so, and never what or how much.
  ///
  /// - Parameters:
  ///   - run: The step's rows.
  ///   - moment: Its app run.
  /// - Returns: The words.
  static func stepWords(_ run: ActivityRun, in moment: ActivityMoment) -> ViewerStepWords {
    stepWords(run.entry, count: run.count, in: moment)
  }

  /// What one row, or a run of `count` like it, did (``stepWords(_:in:)``).
  ///
  /// - Parameters:
  ///   - entry: The row.
  ///   - count: How many times in a row it was done.
  ///   - moment: Its app run.
  /// - Returns: The words.
  static func stepWords(_ entry: ActivityLog.Entry, count: Int, in moment: ActivityMoment) -> ViewerStepWords {
    let counted = count > 1 ? "×\(count)" : ""
    let appOperation = ["open", "open-link", "close", "install", "cleanup"].contains(entry.verb ?? "")
    if entry.operation == "run", entry.actor != .you, !appOperation {
      let verb =
        switch entry.outcome {
        case .running: ViewerWords.runningCommand
        case .failed: ViewerWords.commandFailed
        case .worked, .notSent: ViewerWords.ranCommand
        }
      return ViewerStepWords(verb, count: counted)
    }
    var said = ActivityGrouping.text(entry)
    // The person's own run already says it was them.
    if moment.stream == .you, said.hasPrefix("You ") { said = Words.leadingCapital(String(said.dropFirst(4))) }
    for app in [moment.app?.name, entry.app].compactMap({ $0 }) where said.hasSuffix(" in \(app)") {
      said.removeLast(" in \(app)".count)
      break
    }
    guard let label = entry.label, !label.isEmpty, said.hasSuffix(" \(label)") else {
      return ViewerStepWords(said, count: counted)
    }
    let verb = String(said.dropLast(label.count + 1))
    if !entry.keys.isEmpty {
      let focused = entry.facts?.element?.name ?? ""
      return ViewerStepWords(
        verb,
        joiner: focused.isEmpty ? "" : "in",
        target: focused,
        keys: entry.keys,
        keyName: ViewerWords.keyName(entry.keys),
        count: counted
      )
    }
    if let typed = typedWords(entry.facts, verb: verb, target: label, count: counted) { return typed }
    return ViewerStepWords(verb, target: label, count: counted)
  }

  /// Typing's words from what Atlas kept beside its picture: "Typed a password into **Password**", or "Typed
  /// “Release 0.1.1…” into **Message**"; nil when it kept no text, which leaves the service's words.
  ///
  /// - Parameters:
  ///   - facts: What Atlas kept.
  ///   - verb: The service's verb, such as "Typed into".
  ///   - target: The field it named.
  ///   - count: How many times in a row it was done.
  /// - Returns: The words, or nil.
  static func typedWords(_ facts: ActionFacts?, verb: String, target: String, count: String) -> ViewerStepWords? {
    guard let facts, facts.action == .type, verb.hasSuffix(" into") else { return nil }
    let typed = String(verb.dropLast(" into".count))
    if facts.secure { return ViewerStepWords("\(typed) \(ViewerWords.aPassword) into", target: target, count: count) }
    guard let text = facts.text else { return nil }
    return ViewerStepWords(typed, quote: ViewerWords.shortened(text), joiner: "into", target: target, count: count)
  }

  /// What the same stream did after a step that did not work, as its callout's last sentence.
  ///
  /// - Parameters:
  ///   - followUp: What followed.
  ///   - problem: The step that did not work.
  ///   - moment: Its app run.
  /// - Returns: "The agent tried again, and it worked.", "It failed again." or "Then it scrolled.".
  static func followUpWords(_ followUp: ActivityFollowUp, after problem: ActivityRun, in moment: ActivityMoment)
    -> String
  {
    switch followUp {
    case .workedOnRetry:
      problem.entry.actor == .you ? ViewerWords.youWorkedOnRetry : ViewerWords.workedOnRetry
    case .failedAgain:
      ViewerWords.failedAgain
    case .then(let next):
      ViewerWords.then(stepWords(next, in: moment).text, personal: next.entry.actor == .you)
    }
  }
}
