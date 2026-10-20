/// What the disc beside an account shows.
package enum AccountMark: Equatable, Sendable {
  /// A symbol on a control fill, such as `plus` for a new account.
  case symbol(String)
  /// An account's first letter on an avatar disc.
  case letter(String)
}
