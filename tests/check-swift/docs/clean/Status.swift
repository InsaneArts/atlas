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
    var actor = 0
    actor = local
    _ = actor
  }

  /// Posts the name.
  ///
  /// - Returns: Whether it went.
  package func post() -> Bool {
    /// The request: the name.
    struct Body: Encodable {
      /// The account's short name.
      let name: String
    }
    /// The reply's fields.
    enum CodingKeys: String, CodingKey { case ok }
    _ = Body(name: name)
    return true
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
