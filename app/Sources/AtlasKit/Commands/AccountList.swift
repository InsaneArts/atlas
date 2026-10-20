/// The accounts `atlas switch --list` reports, and which one is the helper.
package struct AccountList: Decodable, Sendable {
  /// One account on this Mac.
  package struct Account: Decodable, Sendable {
    /// The account's short name.
    package let name: String
    /// Whether `atlas` uses it as the helper.
    package let helper: Bool?
  }

  /// The accounts, in the command's order.
  package let accounts: [Account]

  /// The helper's short name: the first account `atlas` marks as the helper.
  package var helper: String? {
    accounts.first { $0.helper == true }?.name
  }
}
