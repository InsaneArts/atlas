/// What some lines of the Activity outline held: how many of each kind of action, in the order each kind was first
/// done, how many actions, screenshots and problems, and the apps they acted in, most often first. A run's and a
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
  /// The apps the lines acted in, by name, the one acted in most often first.
  public var apps: [String]

  /// Counts `runs`.
  ///
  /// - Parameter runs: The lines, newest first.
  public init(_ runs: [ActivityRun]) {
    var kinds: [ActivityAction: Int] = [:]
    var order: [ActivityAction] = []
    var apps: [String: Int] = [:]
    var appOrder: [String] = []
    var actions = 0
    for run in runs.reversed() {
      let kind = ActivityAction(run.entry)
      if kinds[kind] == nil { order.append(kind) }
      kinds[kind, default: 0] += run.count
      actions = ActivityGrouping.addingActions(run.actions, to: actions)
      guard let name = run.entry.app, !name.isEmpty else { continue }
      if apps[name] == nil { appOrder.append(name) }
      apps[name, default: 0] += run.count
    }
    self.kinds = kinds
    self.order = order
    self.actions = actions
    screenshots = runs.filter { $0.entry.operation == ActivityGrouping.screenshotOperation }
      .reduce(0) { $0 + $1.count }
    problems =
      runs.filter { $0.entry.outcome == .failed }
      .count
    self.apps = appOrder.enumerated()
      .sorted { (apps[$0.element] ?? 0, -$0.offset) > (apps[$1.element] ?? 0, -$1.offset) }
      .map(\.element)
  }

  /// What the lines mostly did, and where: what a session's title says.
  public var gist: ActivityGist? {
    ActivityGist(kinds: kinds, app: apps.first)
  }
}
