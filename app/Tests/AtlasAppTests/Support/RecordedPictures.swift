import CoreGraphics
import Observation

@testable import AtlasApp

/// Stands in for the clipboard and the save panel: records the pictures copied and saved, and touches neither.
@MainActor
@Observable
final class RecordedPictures {
  /// The pictures copied, in order.
  var copied: [CGImage] = []
  /// The pictures saved, in order.
  var saved: [CGImage] = []

  /// The outlet the viewer takes.
  var outlet: PictureOutlet {
    PictureOutlet(
      copy: { self.copied.append($0) },
      save: { image in
        self.saved.append(image)
        return true
      }
    )
  }
}
