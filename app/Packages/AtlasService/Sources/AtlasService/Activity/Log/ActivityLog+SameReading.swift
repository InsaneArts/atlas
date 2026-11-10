extension ActivityLog {
  /// Whether `other` lists the very rows and sessions this one does: a later reading of an unchanged log. A row's age
  /// grows with every reading while its start, when the service gives one, stays, so the age of a row with a start
  /// is not compared; a row with only an age has nothing else to place it by, so its age is.
  ///
  /// - Parameter other: Another reading.
  /// - Returns: Whether the two read the same.
  public func isSameReading(as other: ActivityLog) -> Bool {
    sessions == other.sessions && entries.map(Self.placedByStart) == other.entries.map(Self.placedByStart)
  }

  /// A row as a later reading lists it unchanged: without its age, which only grows, when its start places it.
  ///
  /// - Parameter entry: The row.
  /// - Returns: The row to compare.
  private static func placedByStart(_ entry: Entry) -> Entry {
    guard entry.startedAt != nil else { return entry }
    var placed = entry
    placed.ageSeconds = nil
    return placed
  }
}
