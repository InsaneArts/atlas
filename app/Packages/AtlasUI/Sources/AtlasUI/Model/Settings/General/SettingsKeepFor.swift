/// How long pictures of agent actions are kept, as Settings offers it.
public enum SettingsKeepFor: Hashable, Sendable, CaseIterable {
  /// Three days.
  case threeDays
  /// Seven days, Atlas's default.
  case sevenDays
  /// A number of days the person chooses.
  case custom

  /// The days a choice keeps pictures; nil for a custom number.
  public var days: Int? {
    switch self {
    case .threeDays: 3
    case .sevenDays: 7
    case .custom: nil
    }
  }
}
