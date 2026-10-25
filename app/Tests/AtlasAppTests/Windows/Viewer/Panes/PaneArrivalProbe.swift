import SwiftUI

@testable import AtlasUI

/// Reports the environment while the first pane body draws, before any animation completion.
struct PaneArrivalProbe: View {
  /// Receives whether indicators may show during this drawing.
  let read: (Bool) -> Void
  @Environment(\.paneSettled) private var settled

  /// A clear surface that reports its environment as it draws.
  var body: some View {
    read(settled)
    return Color.clear
  }
}
