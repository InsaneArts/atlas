import Foundation

/// How far putting Atlas inside a VM's guest got, kept in the VM's folder so
/// an install asked again goes on from there. Every phase is safe to repeat.
public struct InstallProgress: Codable, Equatable, Sendable {
  /// The install change it belongs to.
  public var change: ChangeID
  /// The last phase done; none before the first.
  public var done: InstallPhase?
  /// The frozen archive's SHA-256, once made.
  public var archive: String?
  /// Whether the station said the guest paired.
  public var paired: Bool
  /// Why the last try stopped short, when it did.
  public var failure: String?
  /// Why it was refused for good, once it was.
  public var refused: String?
  /// Whether its refusal's cleanup is done: the VM quarantined and stopped,
  /// the secret gone from the share.
  public var ended: Bool

  /// A new install for `change`.
  public init(change: ChangeID) {
    self.change = change
    done = nil
    archive = nil
    paired = false
    failure = nil
    refused = nil
    ended = false
  }

  /// Whether `phase` is done.
  public func passed(_ phase: InstallPhase) -> Bool {
    guard let done else { return false }
    return done.rawValue >= phase.rawValue
  }

  /// The progress kept at `file`; none when there is none.
  public static func read(from file: URL) throws -> InstallProgress? {
    guard let data = try PrivateFile.read(file, limit: 16 * 1024) else { return nil }
    return try JSONDecoder()
      .decode(InstallProgress.self, from: data)
  }

  /// Keep it at `file`.
  public func write(to file: URL) throws {
    try PrivateFile.write(try JSONEncoder().encode(self), to: file)
  }
}
