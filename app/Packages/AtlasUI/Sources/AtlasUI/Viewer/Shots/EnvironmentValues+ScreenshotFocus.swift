import SwiftUI

extension EnvironmentValues {
  /// The viewer's shared destination, also used to restore the grid after dismissal.
  @Entry var screenshotFocus: FocusState<ScreenshotFocusTarget?>.Binding? = nil
}
