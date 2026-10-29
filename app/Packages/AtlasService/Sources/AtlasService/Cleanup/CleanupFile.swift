/// A file in the helper whose collected copy is missing or changed, so cleanup keeps the file itself: where it is in
/// the helper, and where relative to the helper's home when it is there.
public struct CleanupFile: Decodable, Equatable, Sendable {
  /// Where it is in the helper.
  public var source: String
  /// Where it is relative to the helper's home; nil when it is elsewhere.
  public var inHome: String?

  /// Creates a file.
  ///
  /// - Parameters:
  ///   - source: Where it is in the helper.
  ///   - inHome: Where it is relative to the helper's home; none by default.
  public init(source: String, inHome: String? = nil) {
    self.source = source
    self.inHome = inHome
  }
}
