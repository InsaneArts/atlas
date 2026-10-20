import AtlasUI

/// What a test saw of the panel while one of its commands ran.
@MainActor
final class PanelWorkObservation {
  /// Whether the panel still read as working.
  var working: Bool?
  /// The feedback line the panel showed.
  var feedback: Feedback?
}
