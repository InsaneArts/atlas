import SwiftUI

extension EnvironmentValues {
  /// Only explicit result opens request target focus; helper polling leaves it untouched.
  @Entry var settingsReveal: SettingsReveal?
}
