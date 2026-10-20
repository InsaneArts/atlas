import Foundation

/// One picture of the helper's desktop, with what it was taken of.
package struct Frame: Sendable, Equatable {
  /// The picture, as PNG data.
  package var png: Data
  /// Identifies this picture to the service (control and input name it).
  package var id: String
  /// Changes whenever the helper's session or display changes.
  package var revision: String
  /// The display, in points.
  package var display: ViewerStatus.Size

  /// Creates a frame.
  ///
  /// - Parameters:
  ///   - png: The picture, as PNG data.
  ///   - id: Identifies this picture to the service.
  ///   - revision: The helper's session and display revision it was taken in.
  ///   - display: The display, in points.
  package init(png: Data, id: String, revision: String, display: ViewerStatus.Size) {
    self.png = png
    self.id = id
    self.revision = revision
    self.display = display
  }
}
