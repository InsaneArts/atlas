/// Where agents work, as the setup's choice lists it.
public enum OnboardingDesktopKind: CaseIterable, Hashable, Sendable {
  /// A hidden second login on this Mac.
  case secondLogin
  /// A virtual Mac on this Mac.
  case vm
  /// A Mac in the cloud.
  case cloud
}
