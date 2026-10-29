import CoreGraphics

extension AppIconStore {
  /// An icon as kept: its picture, its PNG's digest, and whether it is due to be looked up again where its app is
  /// installed.
  nonisolated struct Kept: Sendable {
    /// The icon, decoded.
    var image: CGImage
    /// The PNG's digest, which tells one icon from a changed one.
    var sha: String
    /// Whether ``AppIconCachePolicy/lookAgainDays`` have passed since it was read from its app.
    var looksAgain: Bool
  }
}
