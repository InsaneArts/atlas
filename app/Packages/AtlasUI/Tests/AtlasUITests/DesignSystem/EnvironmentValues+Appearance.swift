import SwiftUI

extension EnvironmentValues {
  /// The values of a view drawn in the light appearance.
  static var light: EnvironmentValues {
    var environment = EnvironmentValues()
    environment.colorScheme = .light
    return environment
  }

  /// The values of a view drawn in the dark appearance.
  static var dark: EnvironmentValues {
    var environment = EnvironmentValues()
    environment.colorScheme = .dark
    return environment
  }
}
