import Foundation

/// The station's record of a VM's removal, as far as Atlas VM reads it:
/// the removal's change and how far it got. While it is there no run of
/// the VM starts.
struct RemovalBarrier: Decodable, Equatable, Sendable {
  /// The removal's change.
  var change: ChangeID
  /// How far the removal got, as the station names its steps.
  var step: String

  /// The step from which no run of the VM lives and only its passwords are
  /// left to delete.
  static let passwords = "passwords"

  /// The longest record read.
  private static let longest = 256 * 1024

  /// The record kept at `file`; none when there is none.
  static func read(from file: URL) throws -> RemovalBarrier? {
    guard let data = try PrivateFile.read(file, limit: longest) else { return nil }
    do {
      return try JSONDecoder()
        .decode(RemovalBarrier.self, from: data)
    } catch {
      throw PrivateFileError.unreadable(file)
    }
  }
}
