/// One row of a ``GroupedList`` of ``ListRow``s that a view lists itself, such as a checklist's lines: its identity and
/// what the row shows.
struct ListRowItem<ID: Hashable & Sendable>: Identifiable, Equatable, Sendable {
  /// What the row stands for, such as the account it picks.
  let id: ID
  /// The title.
  let title: String
  /// The detail under it, or nil for none.
  var detail: String?
  /// What leads the row.
  var lead = ListRow.Lead.none
  /// What ends the row.
  var trailing = ListRow.Trailing.none
}
