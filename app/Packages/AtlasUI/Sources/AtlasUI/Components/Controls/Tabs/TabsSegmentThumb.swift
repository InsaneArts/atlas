import SwiftUI

/// A segment's raised thumb: a white capsule with no rim, raised off the track by the selection's soft shadow, as a
/// pane's filters show the one chosen.
struct TabsSegmentThumb: View {
  /// The thumb, raised off the track.
  var body: some View {
    Capsule()
      .fill(Color.fillSelection)
      .elevation(.selection)
  }
}
