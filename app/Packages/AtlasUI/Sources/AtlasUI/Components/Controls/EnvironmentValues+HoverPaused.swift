import SwiftUI

extension EnvironmentValues {
  /// Whether the pointer's highlights wait, as while the window is being resized, so rows and buttons passing under a
  /// still pointer never flicker; they light again once it is let go, if the pointer is still over them. No by default.
  @Entry var hoverPaused = false
}
