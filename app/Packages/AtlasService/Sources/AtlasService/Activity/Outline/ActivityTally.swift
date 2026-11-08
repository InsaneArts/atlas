/// What some lines of the Activity outline held: how many of each kind of action, in the order each kind was first
/// done, how many actions, screenshots and problems, and the apps they acted in, most often first. A line refused
/// before anything was sent did nothing, so it counts only among the refused. A run's and a
/// session's summaries say it, and a session's title reads its gist.
public struct ActivityTally: Equatable, Sendable {
  /// How many lines did each kind of action.
  public var kinds: [ActivityAction: Int]
  /// The kinds of action, in the order each was first done.
  public var order: [ActivityAction]
  /// How many actions the lines held, repeats and aggregated input included.
  public var actions: Int
  /// How many screenshots were taken.
  public var screenshots: Int
  /// How many lines did not work.
  public var problems: Int
  /// How many actions were refused before anything was sent.
  public var refused: Int
  /// The apps the lines acted in, by name, the one acted in most often first.
  public var apps: [String]
  /// The verbs Atlas wrote for each kind's lines, such as `close` among the window's, so words never claim an app was
  /// opened when it was closed.
  public var verbs: [ActivityAction: Set<String>]

  /// Counts `runs`.
  ///
  /// - Parameter runs: The lines, newest first.
  public init(_ runs: [ActivityRun]) {
    var kinds: [ActivityAction: Int] = [:]
    var order: [ActivityAction] = []
    var apps: [String: Int] = [:]
    var appOrder: [String] = []
    var verbs: [ActivityAction: Set<String>] = [:]
    var actions = 0
    var refused = 0
    for run in runs.reversed() {
      guard run.entry.outcome != .notSent else {
        refused = ActivityGrouping.addingActions(run.actions, to: refused)
        continue
      }
      let kind = ActivityAction(run.entry)
      if kinds[kind] == nil { order.append(kind) }
      kinds[kind, default: 0] += run.count
      if let verb = run.entry.verb { verbs[kind, default: []].insert(verb) }
      actions = ActivityGrouping.addingActions(run.actions, to: actions)
      guard let name = run.entry.app, !name.isEmpty else { continue }
      if apps[name] == nil { appOrder.append(name) }
      apps[name, default: 0] += run.count
    }
    self.kinds = kinds
    self.order = order
    self.verbs = verbs
    self.actions = actions
    self.refused = refused
    screenshots = runs.filter {
      $0.entry.operation == ActivityGrouping.screenshotOperation && $0.entry.outcome != .notSent
    }
    .reduce(0) { $0 + $1.count }
    problems =
      runs.filter { $0.entry.outcome == .failed }
      .count
    self.apps = appOrder.enumerated()
      .sorted { (apps[$0.element] ?? 0, -$0.offset) > (apps[$1.element] ?? 0, -$1.offset) }
      .map(\.element)
  }
}
