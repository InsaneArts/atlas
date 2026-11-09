import AtlasService

/// A held sign-in this window runs: what it is for, its site and the browser it opened.
struct ViewerSignIn: Equatable {
  /// What it is for.
  var kind: SignInKind
  /// The site, such as `github.com`.
  var site: String
  /// The browser it opened, by its bundle identifier.
  var browser: String?
}
