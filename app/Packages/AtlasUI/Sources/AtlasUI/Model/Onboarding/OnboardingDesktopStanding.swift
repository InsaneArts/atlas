/// Whether a row of the choice of where agents work can be picked now.
public enum OnboardingDesktopStanding: Equatable, Sendable {
  /// It can be picked: its radio, on when it is the one picked.
  ///
  /// - Parameter isPicked: Whether it is the one picked.
  case choice(isPicked: Bool)
  /// Its turn has not come: dimmed, with why at its end in place of a radio, and no press.
  ///
  /// - Parameter why: Why, in a few words, such as "Not available yet".
  case notYet(String)
}
