extension ActionFacts {
  /// What kind of input an action sent.
  public enum Action: String, Equatable, Sendable {
    /// A click of the pointer.
    case click
    /// Text typed.
    case type
    /// Keys pressed.
    case key
    /// A scroll.
    case scroll
    /// A drag from one point to another.
    case drag
    /// An element pressed by name, the pointer never moving.
    case press
    /// The pointer moved.
    case move
  }
}
