/// The helper's state, as the panel shows it.
package struct Status: Sendable {
  package let name: String

  /// Creates a status for one account.
  ///
  /// - Parameter name: The account's short name.
  package init(name: String) {
    self.name = name
  }
}

package enum Word {
  case ready
}
