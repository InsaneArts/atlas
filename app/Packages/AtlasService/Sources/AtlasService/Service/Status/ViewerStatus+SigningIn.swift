extension ViewerStatus {
  /// A held sign-in under way in the helper: agents paused and the screen hidden from them.
  public struct SigningIn: Decodable, Equatable, Sendable {
    /// The site.
    public var site: String
    /// What it is for.
    public var kind: SignInKind

    /// Creates a held sign-in.
    ///
    /// - Parameters:
    ///   - site: The site.
    ///   - kind: What it is for.
    public init(site: String, kind: SignInKind) {
      self.site = site
      self.kind = kind
    }
  }
}
