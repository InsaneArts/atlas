extension OnboardingFacts {
  /// The helper, as the person's account sees it.
  package struct Helper: Equatable, Sendable {
    /// Whether the chosen account still exists.
    package var exists = false
    /// Its short name.
    package var name = ""
    /// Its full name, when it has one.
    package var realName: String?
    /// Whether it administers this Mac; unknown when `nil`.
    package var administrator: Bool?
    /// Whether macOS still runs a session of a deleted account with its user ID.
    package var leftoverSession = false
    /// This account's request names this helper.
    package var paired = false
    /// Whether it is logged in.
    package var signedIn = false
    /// Whether Atlas answers in it.
    package var answering = false
    /// Whether its session is the one on screen.
    package var onConsole = false
    /// Whether its session is off screen, as Atlas needs it.
    package var hidden = false
    /// Whether Atlas has Screen Recording there.
    package var screenCapture = false
    /// Whether Atlas has Accessibility there.
    package var accessibility = false
    /// The helper as people know it: its full name, else its short one.
    package var title: String { realName ?? name }

    /// Creates a helper with nothing known.
    package init() {}
  }
}
