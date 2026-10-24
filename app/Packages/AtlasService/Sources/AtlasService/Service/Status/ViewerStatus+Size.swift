extension ViewerStatus {
  /// A display's size, in points.
  public struct Size: Decodable, Equatable, Sendable {
    /// The width, in points.
    public var width: Double
    /// The height, in points.
    public var height: Double

    /// Creates a size.
    ///
    /// - Parameters:
    ///   - width: The width, in points.
    ///   - height: The height, in points.
    public init(width: Double, height: Double) {
      self.width = width
      self.height = height
    }
  }
}
