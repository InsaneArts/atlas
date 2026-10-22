import SwiftUI

extension EnvironmentValues {
  /// Where a screenshot dragged out of the Screenshots pane comes from. Nil, as in previews and renders, drags
  /// nothing.
  @Entry package var shotDrag: ShotDragSource? = nil
}
