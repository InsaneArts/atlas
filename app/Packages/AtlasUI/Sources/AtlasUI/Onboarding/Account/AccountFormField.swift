/// One field of the new helper account's form, in the order the form shows them.
enum AccountFormField: CaseIterable, Identifiable, Sendable {
  /// The account's full name.
  case fullName
  /// Its short name.
  case accountName
  /// The password.
  case password
  /// The password, typed again.
  case verify

  /// The field itself.
  var id: Self { self }

  /// The field's name, in the form's label column.
  var label: String {
    switch self {
    case .fullName: "Full name"
    case .accountName: "Account name"
    case .password: "Password"
    case .verify: "Verify"
    }
  }

  /// Where the form keeps what is typed in it.
  var value: WritableKeyPath<OnboardingAccountForm, String> {
    switch self {
    case .fullName: \.fullName
    case .accountName: \.accountName
    case .password: \.password
    case .verify: \.verify
    }
  }

  /// Whether what is typed is hidden: the passwords'.
  var isSecure: Bool {
    self == .password || self == .verify
  }

  /// What the empty field shows: Required on the passwords, which the form starts without.
  var placeholder: String? {
    isSecure ? "Required" : nil
  }

  /// Whether a problem about `flagged` is about this field.
  ///
  /// - Parameter flagged: The field the problem under the form is about, if any.
  /// - Returns: Whether this field is flagged.
  func isFlagged(_ flagged: OnboardingAccountField?) -> Bool {
    switch (self, flagged) {
    case (.accountName, .accountName), (.verify, .verify): true
    default: false
    }
  }
}
