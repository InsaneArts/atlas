import SwiftUI

/// The sidebar's selection: a raised white capsule with a fine rim, as a source list's chosen row stands on.
struct SidebarSelectionPill: View {
  /// The capsule, raised off the sidebar.
  var body: some View {
    Capsule()
      .fill(Color.fillSelection)
      .overlay(
        CapsuleEdge()
          .strokeBorder(Color.strokeSelected, lineWidth: Stroke.hairline)
      )
      .elevation(.selection)
  }
}
