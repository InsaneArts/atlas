extension Ask {
  /// What the agent would do, as the input service judged it.
  public enum Kind: String, Equatable, Sendable, CaseIterable {
    /// Press Return, or a send key, in a message box.
    case sendKey = "send-key"
    /// Press an element whose name sounds like sending, such as Send or Merge.
    case click
    /// Type text that holds a new line, which may send it.
    case newline
    /// Type into a password field.
    case password
    /// Press an element that has no name.
    case unnamed
    /// Drag, which may move or drop something.
    case drag
    /// Something Atlas can't tell, such as a key outside a text field.
    case unsure
  }
}
