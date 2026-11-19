/// What a making log keeps between two tellings: where making stands, when
/// it was last written, and the most bytes of the download received.
struct MakingLogState: Sendable {
  /// The bytes of a download received, by the size of its whole.
  typealias Floor = (received: Int64, total: Int64)

  /// Where making stands.
  var progress: MakingProgress
  /// When it was last written; none until it is, or since a write failed.
  var written: ContinuousClock.Instant?
  /// The most bytes of the download received, its earlier tries' included.
  var floor: Floor?
}
