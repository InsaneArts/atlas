/// What a held sign-in is for: signing the helper in to a site, deleting a site's data in the helper's browser, or
/// clearing that browser's data. Each holds the helper the same way: agents paused, the screen hidden from them.
public enum SignInKind: String, Codable, Equatable, Sendable, CaseIterable {
  /// Sign In to a Site….
  case signIn
  /// Forget This Site….
  case forget
  /// Reset the Helper's Browser….
  case reset
}
