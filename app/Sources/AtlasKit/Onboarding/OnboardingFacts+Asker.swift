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
    /// Whether the two permissions are known: read now, or kept from an earlier reading while Atlas's job restarts
    /// (to pick up Screen Recording), when a reading has them unknown rather than off.
    package var permissionsKnown = false
    /// The asking account as people know it: its full name, else its short one.
    package var title: String { realName ?? name }

    /// Creates an asker with nothing known.
    package init() {}

    /// This asker, with `previous`'s permissions while its own are unknown, so a restarting job never reads as one
    /// that lost them. While `restarting`, a job that ran before still counts as running, so the window does not ask
    /// for Allow again while launchd brings it back; after that, permissions never read count as off, so the person
    /// can allow them.
    ///
    /// - Parameters:
    ///   - previous: The asker read before.
    ///   - restarting: Whether a job whose permissions are unknown may still be coming back.
    /// - Returns: This asker, its permissions known or carried over.
    package func keepingLastKnown(of previous: Asker, restarting: Bool) -> Asker {
      guard !permissionsKnown else { return self }
      var kept = self
      kept.accessibility = previous.accessibility
      kept.screenCapture = previous.screenCapture
      kept.permissionsKnown = previous.permissionsKnown || !restarting
      if restarting, installed, previous.running { kept.running = true }
      return kept
    }
  }
}
