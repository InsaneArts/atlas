import SwiftUI

extension EnvironmentValues {
  /// The Settings window's invoking button, separate from inert search targets.
  @Entry var settingsAlertFocus: SettingsAlertFocus? = nil
  /// Only alert-opening list controls participate in focus restoration.
  @Entry var settingsAlertControl = false
}
