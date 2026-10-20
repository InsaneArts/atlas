extension ViewerStatus {
  /// A display's size, in points.
  package struct Size: Decodable, Equatable, Sendable {
    /// The width, in points.
    package var width: Double
    /// The height, in points.
    package var height: Double

    /// Creates a size.
    ///
    /// - Parameters:
    ///   - width: The width, in points.
    ///   - height: The height, in points.
    package init(width: Double, height: Double) {
      self.width = width
      self.height = height
    }
  }
}
