import Foundation

/// The Activity pane's moments, all pure: which lines form one moment, which the filters and the search keep, which
/// moments show their steps, and what followed a problem. Moments form from every line in scope before any filter
/// applies, so a moment keeps its identity whichever filter shows it. Time comes in and is never read.
public struct ActivityMoments: Sendable {
  /// 120 s: the longest pause inside a moment, between its oldest line so far and the line before it. An agent's run
  /// of work is a string of actions seconds apart; a longer pause starts a new
  /// moment, even in the same app.
  static let momentGap: TimeInterval = 120
  /// 5 min: how lately a moment must have ended to stay open on its own.
  static let openSpan: TimeInterval = 300
  /// 3: the most moments open by default, the newest included.
  static let mostOpen = 3
  /// 120 s: how soon after a problem the same stream's next line must start to be what it did next.
  static let followUpSpan: TimeInterval = 120
  /// 60: the most moments the pane draws, which the service's 200 rows never pass in practice.
  public static let mostMoments = 60

  /// Every moment in scope, newest first, each with every line.
  public var moments: [ActivityMoment]
  /// Every line in scope, newest first, each with its stream.
  var runs: [(stream: ActivityStream, run: ActivityRun)]

  /// Forms the moments of one reading of the log.
  ///
  /// - Parameters:
  ///   - grouping: The rules over the reading, which say when each row started and on which day.
  ///   - captures: The screenshots the service lists, for the screenshot rows.
  ///   - scope: Whose rows to form moments from.
  ///   - previous: The preceding reading's moments, for retaining identities where their actions overlap.
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

  /// The moments the pane lists: each with only the lines the filter and the search keep, a moment left with none
  /// left out, at most ``mostMoments``. The search matches what a line did, or the moment's app by name, which keeps
  /// each of its lines the filter keeps.
  ///
  /// - Parameters:
  ///   - filter: Which lines to list.
  ///   - query: What to search for; empty keeps every line.
  /// - Returns: The moments, newest first.
  public func shown(filter: ActivityFilter, query: String) -> [ActivityMoment] {
    let words = query.trimmingCharacters(in: .whitespacesAndNewlines)
    let kept = moments.compactMap { moment -> ActivityMoment? in
      let named = !words.isEmpty && (moment.app?.name?.localizedStandardContains(words) ?? false)
      var shown = moment
      shown.runs = moment.runs.filter { run in
        filter.admits(run.entry) && (named || ActivityGrouping.matches(run.entry, query: words))
      }
      return shown.runs.isEmpty ? nil : shown
    }
    return Array(kept.prefix(Self.mostMoments))
  }

  /// Whether each moment shows its steps. A moment with a line under way or the action of unknown outcome, and every
  /// moment under Problems, is held open. Otherwise the person's choice wins; without one, the newest moment and any
  /// that ended within ``openSpan`` are open, at most ``mostOpen`` in all, the held ones counted.
  ///
  /// - Parameters:
  ///   - shown: The moments the pane lists, newest first.
  ///   - filter: The pane's filter.
  ///   - uncertainRow: The id of the row whose outcome is unknown, while one is.
  ///   - choices: The person's own choice for a moment, by its id: true to open it, false to fold it.
  ///   - now: The current time, measured against each moment's end without reforming it.
  /// - Returns: Each moment's fold, by its id.
  public func folds(
    of shown: [ActivityMoment],
    filter: ActivityFilter,
    uncertainRow: String?,
    choices: [String: Bool],
    now: Date
  ) -> [String: ActivityMoment.Fold] {
    var folds: [String: ActivityMoment.Fold] = [:]
    var opened = 0
    for (index, moment) in shown.enumerated() {
      let held =
        filter == .problems || moment.isRunning
        || moment.runs.contains { $0.id == uncertainRow }
      let recent = moment.endedAt.map { now.timeIntervalSince($0) < Self.openSpan } ?? false
      let opens = held || ((index == 0 || recent) && opened < Self.mostOpen)
      if opens { opened += 1 }
      if held {
        folds[moment.id] = .held
      } else if let chosen = choices[moment.id] {
        folds[moment.id] = chosen ? .open : .folded
      } else {
        folds[moment.id] = opens ? .open : .folded
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

  /// The moments the lines form, newest first. A line joins its stream's open moment while it falls on the same day,
  /// lies at most ``momentGap`` before the moment's oldest line, and acts in the same app, in none, or the moment is in
  /// none yet, which then takes the line's app; else it opens a new moment for its stream. A line with no time stays
  /// with the moment before it.
  ///
  /// - Parameter runs: The lines in scope, newest first, each with its day.
  /// - Returns: The moments.
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
