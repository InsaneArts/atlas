extension OnboardingAccountForm: CustomStringConvertible, CustomDebugStringConvertible, CustomReflectable {
  /// The mark printed where the password would be.
  static let redacted = "<redacted>"

  /// The form with both password fields redacted.
  public var description: String {
    "OnboardingAccountForm(fullName: \(fullName.debugDescription), accountName: \(accountName.debugDescription), "
      + "password: \(Self.redacted), verify: \(Self.redacted))"
  }

  /// The same as ``description``, so `String(reflecting:)` never shows the password either.
  public var debugDescription: String { description }

  /// The form's fields for `dump` and debuggers, with both password fields redacted.
  public var customMirror: Mirror {
    Mirror(
      self,
      children: [
        "fullName": fullName,
        "accountName": accountName,
        "password": Self.redacted,
        "verify": Self.redacted,
      ],
      displayStyle: .struct
    )
  }
}
