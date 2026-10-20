import Foundation

/// One of the two permissions Atlas needs in the helper, and where System
/// Settings keeps its switch.
package enum Permission: String, CaseIterable, Identifiable, Sendable {
  /// Accessibility, for input.
  case accessibility
  /// Screen Recording, for pictures.
  case screenRecording

  /// The permission's stable id, as the setup window's events name it; never shown, so a new title leaves it as it
  /// is.
  package var id: String { rawValue }

  /// The permission's name, as System Settings shows it.
  package var title: String {
    switch self {
    case .accessibility: "Accessibility"
    case .screenRecording: "Screen Recording"
    }
  }

  /// "1 of 2", as the guide panel counts them.
  package var number: Int { (Permission.allCases.firstIndex(of: self) ?? 0) + 1 }

  /// The Privacy & Security page that holds the switch.
  package var settingsURL: URL? {
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
  package func granted(to asker: OnboardingFacts.Asker) -> Bool {
    switch self {
    case .accessibility: asker.accessibility
    case .screenRecording: asker.screenCapture
    }
  }
}
