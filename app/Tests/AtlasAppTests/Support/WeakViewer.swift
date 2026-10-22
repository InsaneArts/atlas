@testable import AtlasApp

/// The viewer a test's helper commands end the control of, held weakly as the app's controller holds its window.
@MainActor
final class WeakViewer {
  /// The viewer, while it lives.
  weak var controller: ViewerController?
}
