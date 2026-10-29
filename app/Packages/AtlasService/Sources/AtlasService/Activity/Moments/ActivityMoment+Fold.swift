extension ActivityMoment {
  /// Whether a moment shows its steps or folds to one row.
  public enum Fold: Equatable, Sendable {
    /// Its steps show: by default, or because the person opened it.
    case open
    /// It is one row, by default or because the person folded it.
    case folded
    /// Its steps always show, whatever the person chose: a line is under way, its action's outcome is unknown, or
    /// Problems shows.
    case held

    /// Whether its steps show.
    public var isOpen: Bool { self != .folded }
  }
}
