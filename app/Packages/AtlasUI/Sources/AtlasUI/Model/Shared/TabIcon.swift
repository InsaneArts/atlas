/// What an icon tab draws over its word.
public enum TabIcon: Equatable, Sendable {
  /// An SF Symbol, by name.
  case symbol(String)
  /// Atlas's orbit.
  case orbit
}
