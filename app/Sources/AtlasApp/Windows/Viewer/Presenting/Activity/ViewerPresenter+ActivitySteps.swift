import AtlasService
import AtlasStatus
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// One step of an open moment: its kind and mark, its words without the moment's app, when it was done, the
  /// screenshot it took at its shape with its thumbnail once read, and for a step that did not work why and what the
  /// same stream did next. The action whose outcome is unknown is marked as a problem, says that every session waits,
  /// and offers Show Screen and Recover….
  ///
  /// - Parameters:
  ///   - run: The step's rows.
  ///   - moment: Its moment.
  ///   - moments: Every moment in scope, for what followed a problem.
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
    let mark: ViewerStepMark =
      switch (offersRecovery, entry.outcome) {
      case (true, _), (false, .failed): .failed
      case (false, .running): .live
      case (false, .notSent): .notSent
      case (false, .worked): .quiet
      }
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
      time: run.startedAt.map { stepAge($0, snapshot) } ?? Words.unknown,
      shot: run.shot.map { activityShot($0, snapshot) },
      problem: problem,
      offersRecovery: offersRecovery
    )
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

  /// What a step did, in the service's own words less the moment's app and, in the person's own moment, less "You",
  /// which its header already names: the control
  /// or field it named set apart in semibold, the keys it pressed as caps in place of their label, and the repeats
  /// counted. A command says it ran or failed and never what it ran; nothing typed is ever part of it.
  ///
  /// - Parameters:
  ///   - run: The step's rows.
  ///   - moment: Its moment.
  /// - Returns: The words.
  static func stepWords(_ run: ActivityRun, in moment: ActivityMoment) -> ViewerStepWords {
    let entry = run.entry
    let count = run.count > 1 ? "×\(run.count)" : ""
    let appOperation = ["open", "open-link", "close", "install", "cleanup"].contains(entry.verb ?? "")
    if entry.operation == "run", entry.actor != .you, !appOperation {
      let verb =
        switch entry.outcome {
        case .running: ViewerWords.runningCommand
        case .failed: ViewerWords.commandFailed
        case .worked, .notSent: ViewerWords.ranCommand
        }
      return ViewerStepWords(verb, count: count)
    }
    var said = ActivityGrouping.text(entry)
    // The person's own moment already says it was them.
    if moment.stream == .you, said.hasPrefix("You ") { said = Words.leadingCapital(String(said.dropFirst(4))) }
    for app in [moment.app?.name, entry.app].compactMap({ $0 }) where said.hasSuffix(" in \(app)") {
      said.removeLast(" in \(app)".count)
      break
    }
    guard let label = entry.label, !label.isEmpty, said.hasSuffix(" \(label)") else {
      return ViewerStepWords(said, count: count)
    }
    let verb = String(said.dropLast(label.count + 1))
    if !entry.keys.isEmpty { return ViewerStepWords(verb, keys: entry.keys, count: count) }
    return ViewerStepWords(verb, target: label, count: count)
  }

  /// What the same stream did after a step that did not work, as its callout's last sentence.
  ///
  /// - Parameters:
  ///   - followUp: What followed.
  ///   - problem: The step that did not work.
  ///   - moment: Its moment.
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

  /// A step's screenshot: its name, its shape as the screenshots list says it, and its thumbnail once read.
  ///
  /// - Parameters:
  ///   - name: The capture's file name.
  ///   - snapshot: What the controller holds.
  /// - Returns: The screenshot.
  static func activityShot(_ name: String, _ snapshot: ViewerSnapshot) -> ViewerActivityShot {
    let listed = snapshot.panes.shots.shots?.first { $0.name == name }
    let aspect: CGFloat =
      if let width = listed?.width, let height = listed?.height, width > 0, height > 0 {
        CGFloat(width) / CGFloat(height)
      } else {
        ViewerActivityShot.defaultAspect
      }
    return ViewerActivityShot(name: name, aspect: aspect, thumbnail: snapshot.panes.thumbnails[name])
  }
}
