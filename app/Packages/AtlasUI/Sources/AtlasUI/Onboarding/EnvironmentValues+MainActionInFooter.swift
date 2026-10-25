import SwiftUI

extension EnvironmentValues {
  /// Whether the setup footer holds every screen's main action: true. False hands Get Started, Lock and
  /// Switch, Try Again, Check Again, Allow and Switch Back back to the screen, drawn above the footer as a large
  /// button the way the screens drew them before: a way back, should people look for those buttons in the screen.
  @Entry public var mainActionInFooter = true
}
