import Foundation
import Observation

/// The URLs a controller asked to open, so no test opens one.
@MainActor
@Observable
final class OpenedURLs {
  /// In the order asked.
  var urls: [URL] = []
}
