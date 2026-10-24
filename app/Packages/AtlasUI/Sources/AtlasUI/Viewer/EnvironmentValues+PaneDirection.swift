import SwiftUI

extension EnvironmentValues {
  /// Which way the last tab switch went, from ``ViewerTab/direction(from:to:)``: 1 to the right, −1 back, 0 before
  /// any; the stage's panes read it as they come and go, so the one leaving moves the same way as the one arriving.
  @Entry var paneDirection = 0
}
