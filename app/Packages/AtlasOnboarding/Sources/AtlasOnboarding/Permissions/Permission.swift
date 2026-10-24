import Foundation

/// One of the two permissions Atlas needs in the helper, and where System
/// Settings keeps its switch.
public enum Permission: String, CaseIterable, Identifiable, Sendable {
  /// Accessibility, for input.
  case accessibility
  /// Screen Recording, for pictures.
  case screenRecording

  /// The permission's stable id, as the setup window's events name it; never shown, so a new title leaves it as it
  /// is.
  public var id: String { rawValue }

  /// The permission's name, as System Settings shows it on macOS `version`: Screen Recording became Screen & System
  /// Audio Recording in macOS 15, and Accessibility became Device Control and Data Access in macOS 27.
  ///
  /// - Parameter version: The macOS version.
  /// - Returns: The page's name.
  public func title(on version: OperatingSystemVersion) -> String {
    switch self {
    case .accessibility: version.majorVersion >= 27 ? "Device Control and Data Access" : "Accessibility"
    case .screenRecording: version.majorVersion >= 15 ? "Screen & System Audio Recording" : "Screen Recording"
    }
  }

  /// "1 of 2", as the permission panel counts them.
  public var number: Int { (Permission.allCases.firstIndex(of: self) ?? 0) + 1 }

  /// The Privacy & Security page that holds the switch.
  public var settingsURL: URL? {
    let anchor =
      switch self {
      case .accessibility: "Privacy_Accessibility"
      case .screenRecording: "Privacy_ScreenCapture"
      }
    return URL(string: "x-apple.systempreferences:com.apple.preference.security?\(anchor)")
  }

  /// Whether `asker` (this account, in the helper) has it.
  ///
  /// - Parameter asker: This account's state, in the helper.
  /// - Returns: Whether the permission is granted.
  public func granted(to asker: OnboardingFacts.Asker) -> Bool {
    switch self {
    case .accessibility: asker.accessibility
    case .screenRecording: asker.screenCapture
    }
  }
}
