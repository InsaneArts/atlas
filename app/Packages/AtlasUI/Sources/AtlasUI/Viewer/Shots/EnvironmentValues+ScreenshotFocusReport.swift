import SwiftUI

extension EnvironmentValues {
  /// Tells the native shell whether screenshot content currently owns keyboard focus.
  @Entry public var screenshotFocusReport: ScreenshotFocusReport? = nil
}
