extension OnboardingFacts {
  /// The helper, as the person's account sees it.
  public struct Helper: Equatable, Sendable {
    /// Whether the chosen account still exists.
    var exists = false
    /// Its short name.
    public var name = ""
    /// Its full name, when it has one.
    var realName: String?
    /// Whether it administers this Mac; unknown when `nil`.
    var administrator: Bool?
    /// Whether macOS still runs a session of a deleted account with its user ID.
    var leftoverSession = false
    /// This account asked this helper: its request names it, or a setup with it is recorded.
    var paired = false
    /// This account asked it, and it never answered: the request may be taken back to choose another account.
    public var forgettable = false
    /// Whether it is logged in.
    public var signedIn = false
    /// Whether Atlas answers in it.
    public var answering = false
    /// Whether its session is the one on screen.
    public var onConsole = false
    /// Whether its session is off screen, as Atlas needs it.
    var hidden = false
    /// Whether Atlas has Screen Recording there.
    var screenCapture = false
    /// Whether Atlas has Accessibility there.
    var accessibility = false
    /// The helper as people know it: its full name, else its short one.
    public var title: String { realName ?? name }

    /// Creates a helper with nothing known.
    init() {}
  }
}
