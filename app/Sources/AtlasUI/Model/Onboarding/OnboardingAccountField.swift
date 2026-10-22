/// A field of the account form that a problem points at, so the form can mark it.
package enum OnboardingAccountField: Equatable, Sendable {
  /// The short name.
  case accountName
  /// The password typed again.
  case verify
}
