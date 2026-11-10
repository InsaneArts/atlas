/// A held sign-in the service started: the person's lease, which input, renewals and the end name, how long it lasts
/// unrenewed, and the site and browser it opened.
public struct SignInStarted: Decodable, Equatable, Sendable {
  /// The person's lease.
  public var lease: String
  /// How long the lease lasts unrenewed, in seconds.
  public var ttlSeconds: Double
  /// What the hold is for.
  public var kind: SignInKind
  /// The site, as the accounts list names it, such as `github.com`.
  public var site: String
  /// The browser it opened, by its bundle identifier.
  public var browser: String?

  /// Creates a started sign-in.
  ///
  /// - Parameters:
  ///   - lease: The person's lease.
  ///   - ttlSeconds: How long it lasts unrenewed; 10 by default.
  ///   - kind: What it is for.
  ///   - site: The site.
  ///   - browser: The browser it opened; none by default.
  public init(lease: String, ttlSeconds: Double = 10, kind: SignInKind, site: String, browser: String? = nil) {
    self.lease = lease
    self.ttlSeconds = ttlSeconds
    self.kind = kind
    self.site = site
    self.browser = browser
  }
}
