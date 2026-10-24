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

    /// A filled field and an empty secure one, by label and value.
    static let fields = [
      SampleListItem(id: 0, title: String("Full name"), detail: String("Atlas")),
      SampleListItem(id: 1, title: String("Password")),
    ]
  }
#endif
