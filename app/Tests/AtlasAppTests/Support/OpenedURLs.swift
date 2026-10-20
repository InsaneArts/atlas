import Foundation

/// The URLs a controller asked to open, so no test opens one.
@MainActor
final class OpenedURLs {
  /// In the order asked.
  var urls: [URL] = []
}
