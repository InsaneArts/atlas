import Foundation

/// What asking Apple's framework for the newest macOS it supports came to,
/// kept with when it was asked: the release, or none when it could not be
/// told.
struct LatestKept: Codable, Equatable, Sendable {
  /// The release it named; none when the asking failed.
  var release: MacOSRelease?
  /// When it was asked.
  var asked: Date

  /// The answer kept at `file`; none when there is none.
  static func read(from file: URL) throws -> LatestKept? {
    guard let data = try PrivateFile.read(file, limit: 4 * 1024) else { return nil }
    return try JSONDecoder()
      .decode(LatestKept.self, from: data)
  }

  /// Keep it at `file`.
  func write(to file: URL) throws {
    try PrivateFile.write(try JSONEncoder().encode(self), to: file)
  }
}
