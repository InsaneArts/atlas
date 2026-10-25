import SwiftUI

extension EnvironmentValues {
  /// Whether the pane a view is in has finished fading in (``KeptPaneShift``): false while it crossfades with the pane
  /// it replaces, so a scroll view keeps its indicators hidden until then.
  @Entry var paneSettled = true
}
