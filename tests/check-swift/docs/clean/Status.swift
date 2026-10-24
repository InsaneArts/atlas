/// The helper's state, as the panel shows it.
package struct Status: Sendable {
  /// The account's short name.
  package let name: String
  /// Whether the helper is ready to act.
  package var isReady = false
  private let secret = ""

  /// Creates a status for one account.
  ///
  /// - Parameter name: The account's short name.
  package init(name: String) {
    self.name = name
    let local = name.count
    _ = local
  }

  /// What the panel says, in a word.
  package enum Word {
    /// Ready to act.
    case ready
  }
}

/// A private type's members need no comment of their own.
private struct Scratch {
  let value = 1
}
