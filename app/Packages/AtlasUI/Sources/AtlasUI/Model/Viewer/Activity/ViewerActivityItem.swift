/// One row of the Activity list, which draws as one flat list keyed by each row's identity, so a row in two filters
/// stays where it is and an opened moment's steps arrive under it.
public enum ViewerActivityItem: Equatable, Sendable, Identifiable {
  /// A quiet heading over the moments of a day before today.
  ///
  /// - Parameter day: The heading.
  case day(ViewerActivityDay)
  /// A moment's header, open over its steps or folded into one row.
  ///
  /// - Parameter moment: The moment.
  case moment(ViewerActivityMoment)
  /// One step of an open moment.
  ///
  /// - Parameter step: The step.
  case step(ViewerActivityStep)
  /// "Show N more" under an open moment's newest steps.
  ///
  /// - Parameter more: What it shows.
  case more(ViewerMoreSteps)

  /// The row's identity: `day.`, `moment.`, `step.` or `more.` and what it stands for.
  public var id: String {
    switch self {
    case .day(let day): "day.\(day.id)"
    case .moment(let moment): "moment.\(moment.id)"
    case .step(let step): "step.\(step.id)"
    case .more(let more): "more.\(more.moment)"
    }
  }
}
