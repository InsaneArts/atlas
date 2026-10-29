/// One element of the Activity list as it draws: a quiet heading over a day before today, or a moment with the rows it
/// shows under its header, its steps and "Show N more", so the moment opens and folds as one.
enum ActivityListBlock: Equatable, Identifiable {
  /// A day's heading.
  ///
  /// - Parameter day: The heading.
  case day(ViewerActivityDay)
  /// A moment and the rows under its header, none while it is folded.
  ///
  /// - Parameters:
  ///   - moment: The moment.
  ///   - rows: Its steps and "Show N more", in order.
  case moment(ViewerActivityMoment, rows: [ViewerActivityItem])

  /// The element's identity, its heading's or its moment's row identity, so it is known across filters.
  var id: String {
    switch self {
    case .day(let day): ViewerActivityItem.day(day).id
    case .moment(let moment, _): ViewerActivityItem.moment(moment).id
    }
  }

  /// The list's rows gathered under their moments: each day's heading on its own, and each moment with the steps and
  /// "Show N more" that follow it; a step before any moment is left out.
  ///
  /// - Parameter items: The rows, in order.
  /// - Returns: The elements, in order.
  static func blocks(_ items: [ViewerActivityItem]) -> [ActivityListBlock] {
    var blocks: [ActivityListBlock] = []
    for item in items {
      switch item {
      case .day(let day):
        blocks.append(.day(day))
      case .moment(let moment):
        blocks.append(.moment(moment, rows: []))
      case .step, .more:
        guard case .moment(let moment, let rows)? = blocks.last else { continue }
        blocks[blocks.count - 1] = .moment(moment, rows: rows + [item])
      }
    }
    return blocks
  }
}
