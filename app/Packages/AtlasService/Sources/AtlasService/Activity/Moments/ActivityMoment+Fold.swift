extension ActivityMoment {
  /// Whether a run shows its steps or folds to its row, problems and pictures.
  public enum Fold: Equatable, Sendable {
    /// Its steps show: by default, or because the person opened it.
    case open
    /// It is folded, by default or because the person folded it.
    case folded
    /// Its steps always show, whatever the person chose: its action's outcome is unknown.
    case held

    /// Whether its steps show.
    public var isOpen: Bool { self != .folded }
  }
}
