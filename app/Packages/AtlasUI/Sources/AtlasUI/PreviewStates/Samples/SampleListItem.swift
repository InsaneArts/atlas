#if DEBUG
  /// One row of a list sample: its words and what leads and ends it.
  struct SampleListItem: Identifiable, Sendable {
    /// The row's place in its sample.
    let id: Int
    /// The title.
    let title: String
    /// The detail, or nil.
    var detail: String?
    /// What leads the row.
    var lead = ListRow.Lead.none
    /// What ends the row.
    var trailing = ListRow.Trailing.none
  }
#endif
