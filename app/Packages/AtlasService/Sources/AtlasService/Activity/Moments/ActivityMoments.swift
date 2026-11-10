import Foundation

/// The Activity outline's runs, all pure: which lines form one run, which the filters and the search keep, which
/// runs show their steps, and what followed a problem. Runs form from every line in scope before any filter
/// applies, so a run keeps its identity whichever filter shows it. Time comes in and is never read.
public struct ActivityMoments: Sendable {
  /// 120 s: the longest pause inside a run, between its oldest line so far and the line before it. An agent's work is
  /// a string of actions seconds apart; a longer pause starts a new run, even in the same app.
  static let momentGap: TimeInterval = 120
  /// 120 s: how soon after a problem the same stream's next line must start to be what it did next.
  static let followUpSpan: TimeInterval = 120
  /// 60: the most runs the pane draws, which the service's 200 rows never pass in practice.
  public static let mostMoments = 60

  /// Every run in scope, newest first, each with every line.
  public var moments: [ActivityMoment]
  /// Every line in scope, newest first, each with its stream.
  var runs: [(stream: ActivityStream, run: ActivityRun)]

  /// Forms the runs of one reading of the log.
  ///
  /// - Parameters:
  ///   - grouping: The rules over the reading, which say when each row started and on which day.
  ///   - captures: The screenshots the service lists, for the screenshot rows.
  ///   - scope: Whose rows to form runs from.
  ///   - previous: The preceding reading's runs, for retaining identities where their actions overlap.
  public init(
    _ grouping: ActivityGrouping,
    captures: [Screenshot],
    scope: ActivityScope,
    preserving previous: [ActivityMoment] = []
  ) {
    let calendar = grouping.calendar
    let days = grouping.runs(captures: captures, first: calendar.startOfDay(for: grouping.now)) {
      calendar.startOfDay(for: $0)
    }
    .filter { scope.admits($0.run.entry) }
    runs = days.map { (ActivityStream($0.run.entry), $0.run) }
    moments = Self.formed(days)
    Self.reconcile(&moments, preserving: previous)
  }

  /// Reserves inherited identities before naming split fragments. A fragment's oldest action can have seeded an
  /// identity now inherited by another fragment, so its seed alone is no longer necessarily available.
  private static func reconcile(_ moments: inout [ActivityMoment], preserving previous: [ActivityMoment]) {
    var available = previous
    var inherited: Set<Int> = []
    var reserved: Set<String> = []
    for index in moments.indices {
      let moment = moments[index]
      let actions = Set(moment.runs.flatMap(\.logicalIDs))
      let matches = available.indices.filter {
        available[$0].stream == moment.stream && available[$0].day == moment.day
          && !reserved.contains(available[$0].id)
          && !actions.isDisjoint(with: available[$0].runs.flatMap(\.logicalIDs))
      }
      guard
        let match = matches.max(by: {
          actions.intersection(available[$0].runs.flatMap(\.logicalIDs)).count
            < actions.intersection(available[$1].runs.flatMap(\.logicalIDs)).count
        })
      else { continue }
      moments[index].id = available.remove(at: match).id
      inherited.insert(index)
      reserved.insert(moments[index].id)
    }
    for index in moments.indices where !inherited.contains(index) {
      let seed = moments[index].id
      var identity = seed
      var fragment = 1
      while !reserved.insert(identity).inserted {
        identity = "\(seed)|fragment:\(fragment)"
        fragment += 1
      }
      moments[index].id = identity
    }
  }

  /// The run in scope with an identity.
  ///
  /// - Parameter id: The run's identity.
  /// - Returns: The run, or nil once it is gone.
  public func moment(_ id: String) -> ActivityMoment? {
    moments.first { $0.id == id }
  }

  /// The runs the pane lists: each with only the lines the filter and the search keep, a run left with none
  /// left out, at most ``mostMoments``. The search matches what a line did, or the run's app by name, which keeps
  /// each of its lines the filter keeps.
  ///
  /// - Parameters:
  ///   - filter: Which lines to list.
  ///   - query: What to search for; empty keeps every line.
  /// - Returns: The runs, newest first.
  public func shown(filter: ActivityFilter, query: String) -> [ActivityMoment] {
    let words = query.trimmingCharacters(in: .whitespacesAndNewlines)
    let kept = moments.compactMap { moment -> ActivityMoment? in
      // A run titled by its app or its one listed account keeps every line when the search names it.
      let named =
        !words.isEmpty
        && [moment.app?.name, moment.account?.name].contains { $0?.localizedStandardContains(words) == true }
      var shown = moment
      shown.runs = moment.runs.filter { run in
        filter.admits(run.entry) && (named || ActivityGrouping.matches(run.entry, query: words))
      }
      return shown.runs.isEmpty ? nil : shown
    }
    return Array(kept.prefix(Self.mostMoments))
  }

  /// Whether each run shows its steps. A run holding the action of unknown outcome is held open, so its recovery
  /// shows. Otherwise the person's choice wins; without one, a run folds to its summary, problems and pictures under
  /// All, and opens under any other filter, whose steps are what the person asked to see.
  ///
  /// - Parameters:
  ///   - shown: The runs the pane lists, newest first.
  ///   - filter: The pane's filter.
  ///   - uncertainRow: The id of the row whose outcome is unknown, while one is.
  ///   - choices: The person's own choice for a run, by its id: true to open it, false to fold it.
  /// - Returns: Each run's fold, by its id.
  public func folds(
    of shown: [ActivityMoment],
    filter: ActivityFilter,
    uncertainRow: String?,
    choices: [String: Bool]
  ) -> [String: ActivityMoment.Fold] {
    var folds: [String: ActivityMoment.Fold] = [:]
    for moment in shown {
      if moment.runs.contains(where: { $0.id == uncertainRow }) {
        folds[moment.id] = .held
      } else if let chosen = choices[moment.id] {
        folds[moment.id] = chosen ? .open : .folded
      } else {
        folds[moment.id] = filter == .all ? .folded : .open
      }
    }
    return folds
  }

  /// What the same stream did next after a line that did not work: its next newer line, when that started within
  /// ``followUpSpan`` of the problem's end.
  ///
  /// - Parameter problem: A line that failed or was not sent.
  /// - Returns: The follow-up, or nil for a line that worked, one nothing has followed yet, or one whose retry is
  ///   still under way.
  public func followUp(to problem: ActivityRun) -> ActivityFollowUp? {
    guard [.failed, .notSent].contains(problem.entry.outcome),
      let index = runs.firstIndex(where: { $0.run.id == problem.id })
    else { return nil }
    let stream = runs[index].stream
    guard let next = runs[..<index].last(where: { $0.stream == stream })?.run else { return nil }
    if let started = next.startedAt, let ended = problem.endedAt,
      started.timeIntervalSince(ended) > Self.followUpSpan
    {
      return nil
    }
    return ActivityFollowUp(next: next, after: problem)
  }

  /// The runs the lines form, newest first. A line joins its stream's open run while it falls on the same day,
  /// lies at most ``momentGap`` before the run's oldest line, and acts in the same app, in none, or the run is in
  /// none yet, which then takes the line's app; else it opens a new run for its stream. A line with no time stays
  /// with the run before it.
  ///
  /// - Parameter runs: The lines in scope, newest first, each with its day.
  /// - Returns: The runs.
  static func formed(_ runs: [(part: Date, run: ActivityRun)]) -> [ActivityMoment] {
    var moments: [Forming] = []
    var open: [ActivityStream: Int] = [:]
    for (day, run) in runs {
      let stream = ActivityStream(run.entry)
      let app = ActivityApp(run.entry)
      if let index = open[stream], moments[index].takes(run, app: app, day: day) {
        moments[index].join(run, app: app)
      } else {
        open[stream] = moments.count
        moments.append(Forming(stream: stream, day: day, run: run, app: app))
      }
    }
    return moments.map(\.moment)
  }
}
