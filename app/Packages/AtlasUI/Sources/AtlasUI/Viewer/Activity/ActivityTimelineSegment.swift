import SwiftUI

/// A moment's line through one of its rows, centred under the moment's icon: the whole row's height, or for the
/// moment's last step from the row's top to its disc's centre, where the line ends. Rows sit edge to edge, so their
/// segments join into one line.
struct ActivityTimelineSegment: View {
  /// Whether the line ends at this row's middle.
  var endsHere = false

  /// The line, its upper half and, unless it ends here, its lower half.
  var body: some View {
    VStack(spacing: .zero) {
      line
      if endsHere {
        Color.clear
      } else {
        line
      }
    }
    .frame(width: ViewerLayout.appIcon)
    .accessibilityHidden(true)
  }

  /// Half of the line, round at its ends.
  private var line: some View {
    Capsule()
      .fill(Color.separator)
      .frame(width: ViewerLayout.timeline)
      .frame(maxHeight: .infinity)
  }
}
