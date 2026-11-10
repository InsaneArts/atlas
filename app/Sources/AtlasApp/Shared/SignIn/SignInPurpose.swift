import AtlasService

/// What the person holds the helper for, as Settings asks it: to sign in to a site, to sign out of one on its own page,
/// to delete a site's data, or to clear the browser's. Signing out holds as signing in does, opening the site.
enum SignInPurpose: String, Equatable, Sendable {
  /// Sign In to a Site….
  case signIn
  /// Sign Out…: the site's own page, where the person uses its sign-out.
  case signOut
  /// Forget…: the browser's settings for the site's data.
  case forget
  /// Reset…: the browser's settings for all its data.
  case reset

  /// The hold the service starts for it.
  var kind: SignInKind {
    switch self {
    case .signIn, .signOut: .signIn
    case .forget: .forget
    case .reset: .reset
    }
  }

  /// Whether Done asks to take the site off the accounts too, as after signing out or clearing its data.
  var asksToRemove: Bool {
    self != .signIn
  }
}
