extension OnboardingFacts {
  /// In the helper: the account that asked (unknown when a terminal set
  /// it up), and this account's own state.
  package struct Asker: Equatable, Sendable {
    /// The asking account's user ID; 0 when unknown.
    package var uid = 0
    /// The asking account's short name; empty when no one asked.
    package var name = ""
    /// The asking account's full name, when it has one.
    package var realName: String?
    /// Whether Atlas's job is installed in this account.
    package var installed = false
    /// Whether that job runs.
    package var running = false
    /// Whether Atlas has Screen Recording here.
    package var screenCapture = false
    /// Whether Atlas has Accessibility here.
    package var accessibility = false
    /// The asking account as people know it: its full name, else its short one.
    package var title: String { realName ?? name }

    /// Creates an asker with nothing known.
    package init() {}
  }
}
