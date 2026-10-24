import AtlasUI
import SwiftUI

@testable import AtlasUI

/// Hosts the screenshots with a stable namespace as its state changes under a native scroll view.
struct ShotScrollHost: View {
  /// The screenshot grid's state.
  let state: ViewerShotsState
  @Namespace private var zoom

  /// The grid, with no commands reaching outside the test.
  var body: some View {
    ViewerShotsPane(state: state, zoom: zoom) { _ in }
  }
}
