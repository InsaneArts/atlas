import Foundation

/// How far making a VM got, as its station reads it: the make change it
/// belongs to, its phase, the bytes of a download received of its whole,
/// and the fraction of the phase done. Each is none while it is not known.
public struct MakingProgress: Codable, Equatable, Sendable {
  /// The make change it belongs to.
  public var change: ChangeID
  /// The phase it is in.
  public var phase: MakingPhase
  /// The bytes of the download received, its earlier tries' included.
  public var received: Int64?
  /// The bytes of the whole download.
  public var total: Int64?
  /// How much of the phase is done, from 0 to 1.
  public var fraction: Double?

  /// The make `change` in `phase`, nothing of it counted yet.
  public init(change: ChangeID, phase: MakingPhase) {
    self.change = change
    self.phase = phase
    received = nil
    total = nil
    fraction = nil
  }

  /// The progress kept at `file`; none when there is none.
  public static func read(from file: URL) throws -> MakingProgress? {
    guard let data = try PrivateFile.read(file, limit: 4 * 1024) else { return nil }
    return try JSONDecoder()
      .decode(MakingProgress.self, from: data)
  }

  /// Keep it at `file`.
  public func write(to file: URL) throws {
    try PrivateFile.write(try JSONEncoder().encode(self), to: file)
  }
}
