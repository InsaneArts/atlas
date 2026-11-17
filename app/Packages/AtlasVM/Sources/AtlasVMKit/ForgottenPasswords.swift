import Foundation

/// What the deletion-only entry made of a VM's passwords, under one
/// removal: kept beside that removal's record, for the station to read
/// back.
public struct ForgottenPasswords: Codable, Equatable, Sendable {
  /// The removal it was run under.
  let change: ChangeID
  /// What became of each password, by its Keychain account.
  public let passwords: [String: PasswordFate]

  /// The longest result read.
  private static let longest = 16 * 1024

  /// The result kept at `file`; none when there is none.
  static func read(from file: URL) throws -> ForgottenPasswords? {
    guard let data = try PrivateFile.read(file, limit: longest) else { return nil }
    do {
      return try JSONDecoder()
        .decode(ForgottenPasswords.self, from: data)
    } catch {
      throw PrivateFileError.unreadable(file)
    }
  }

  /// Keep it at `file`, so it outlives a crash.
  func write(to file: URL) throws {
    guard let data = try? JSONEncoder().encode(self), data.count <= Self.longest else {
      throw PrivateFileError.unwritable(file)
    }
    try PrivateFile.write(data, to: file)
  }
}
