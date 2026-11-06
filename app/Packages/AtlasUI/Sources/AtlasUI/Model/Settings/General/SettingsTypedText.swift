/// How much of what an agent typed Atlas keeps beside its pictures, as Settings offers it.
public enum SettingsTypedText: Hashable, Sendable, CaseIterable {
  /// All of it.
  case full
  /// Up to a number of characters the person chooses.
  case limited
}
