import SwiftUI

/// The selection a pill row stands on: a raised white capsule with a fine rim, as a source list's chosen row stands
/// on.
struct PillSelection: View {
  /// The capsule, raised off the ground.
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
