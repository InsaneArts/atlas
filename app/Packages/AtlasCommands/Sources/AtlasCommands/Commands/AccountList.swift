/// The accounts `atlas switch --list` reports, and which one is the helper.
public struct AccountList: Decodable, Sendable {
  /// One account on this Mac.
  struct Account: Decodable, Sendable {
    /// The account's short name.
    let name: String
    /// Whether `atlas` uses it as the helper.
    let helper: Bool?
  }

  /// The accounts, in the command's order.
  let accounts: [Account]

  /// The helper's short name: the first account `atlas` marks as the helper.
  public var helper: String? {
    accounts.first { $0.helper == true }?.name
  }
}
