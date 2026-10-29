/// One line of the sidebar's sessions under All sessions, as one flat list draws them: a section's heading or a
/// session's row. Each keeps one identity as the sessions change, so a row moving from Working to Idle glides to its
/// new place rather than leaving one and arriving at the other; under Reduce Motion a row's identity holds its section
/// too, so it fades out where it was and in where it lands instead.
enum SidebarSessionItem: Equatable, Identifiable {
  /// A section's heading.
  ///
  /// - Parameter section: The section.
  case heading(ViewerSessionSection)
  /// A session's row in its section.
  ///
  /// - Parameters:
  ///   - row: The session.
  ///   - section: The section it is drawn in.
  ///   - keyedBySection: Whether its identity holds its section, as under Reduce Motion.
  case row(ViewerSessionRow, section: ViewerSessionSection, keyedBySection: Bool)

  /// The line's identity: its section's for a heading, its session's for a row, with its section under Reduce Motion.
  var id: String {
    switch self {
    case .heading(let section):
      "heading.\(section.rawValue)"
    case .row(let row, let section, let keyedBySection):
      keyedBySection ? "\(section.rawValue).\(row.id)" : "row.\(row.id)"
    }
  }

  /// The lines `state` draws, in order: each section's heading, then its rows; Ended's heading over the rows it draws.
  ///
  /// - Parameters:
  ///   - state: The sessions.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The lines.
  static func items(_ state: ViewerSessionsState, reduceMotion: Bool) -> [Self] {
    state.groups.flatMap { group in
      [.heading(group.section)]
        + group.rows.map { .row($0, section: group.section, keyedBySection: reduceMotion) }
    }
  }
}
