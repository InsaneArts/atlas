/// The groups of a VM's page in Settings, in order.
public enum VMPageSection: CaseIterable, Equatable, Sendable {
  /// Its power and its maintenance: what the person does with it.
  case use
  /// Its disk, its macOS and its Atlas.
  case about
  /// Removing it.
  case removal
}
