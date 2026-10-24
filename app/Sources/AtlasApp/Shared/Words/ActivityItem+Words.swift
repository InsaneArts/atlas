import AtlasStatus

extension ActivityItem {
  /// What was done, in words (``Words/describe(_:)``).
  var text: String { Words.describe(row) }
  /// How long ago, in words; empty when unknown.
  var when: String { age.map(Words.ago) ?? "" }
  /// Who did what, in one line.
  var line: String { "\(actor.word) \(text)" }
}
