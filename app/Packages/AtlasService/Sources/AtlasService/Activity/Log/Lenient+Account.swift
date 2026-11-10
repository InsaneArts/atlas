extension KeyedDecodingContainer {
  /// The listed account a row's action acted in, or nil when the field is missing, null or does not read whole.
  ///
  /// - Parameter key: The field.
  /// - Returns: The account, or nil.
  func lenientAccount(_ key: Key) -> ActivityLog.Account? {
    try? decodeIfPresent(ActivityLog.Account.self, forKey: key)
  }

  /// What let a row's action act in its account, or nil when the field is missing, null, of the wrong type or a word
  /// this app does not know.
  ///
  /// - Parameter key: The field.
  /// - Returns: The allowance, or nil.
  func lenientAllowance(_ key: Key) -> ActivityLog.Allowance? {
    lenientText(key)
      .flatMap(ActivityLog.Allowance.init(rawValue:))
  }
}
