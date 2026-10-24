/// Whether the taught projects have been read successfully.
public enum SettingsProjectsReading: Equatable, Sendable {
  /// A reading is pending; any rows are from the previous reading.
  case reading
  /// The latest reading succeeded, including an empty list.
  case loaded
  /// The latest reading failed; any rows are from the previous reading.
  case failed
}
