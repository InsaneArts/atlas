/// The helper's state, as the panel shows it.
package struct Status: Sendable {
  package let name: String

  /// Creates a status for one account.
  ///
  /// - Parameter name: The account's short name.
  package init(name: String) {
    self.name = name
  }

  /// Posts the name.
  package func post() {
    struct Body: Encodable {
      let name: String
    }
    /// The reply: whether it went.
    struct Reply: Decodable { let ok: Bool }
    _ = Body(name: name)
  }
}

package enum Word {
  case ready
}
