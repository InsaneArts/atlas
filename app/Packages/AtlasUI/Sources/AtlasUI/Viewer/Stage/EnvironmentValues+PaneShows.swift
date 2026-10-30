import SwiftUI

extension EnvironmentValues {
  /// Whether the pane a view is in shows: false while a kept pane waits hidden behind another pane
  /// (``KeptPaneShift``). A view that asks for something as it comes into sight, such as a thumbnail, asks again as
  /// its pane shows again, since a kept pane's views do not appear anew.
  @Entry var paneShows = true
}
