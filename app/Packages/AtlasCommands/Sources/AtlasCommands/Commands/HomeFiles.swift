import AtlasService

/// Whether the person's home folder is private from the helper, as `atlas onboarding files` says, with what the
/// helper can open there while it isn't, whether keeping it private was offered, and whether agents may copy
/// sign-ins in.
public struct HomeFiles: Equatable, Sendable, Decodable {
  /// Whether the helper is shut out of the home folder.
  public let `private`: Bool
  /// How many folders in the home folder the helper can open; at least this many.
  public let readable: Int
  /// The first of those folders, by name, visible ones first.
  public let examples: [String]
  /// Whether keeping the home folder private was offered for this helper.
  public let offered: Bool
  /// Whether agents may copy sign-ins and secrets into the helper.
  public let signInCopies: Bool

  /// Creates what Atlas says of the home folder.
  ///
  /// - Parameters:
  ///   - isPrivate: Whether the helper is shut out of the home folder.
  ///   - readable: How many folders the helper can open there.
  ///   - examples: The first of those folders, by name.
  ///   - offered: Whether keeping it private was offered.
  ///   - signInCopies: Whether agents may copy sign-ins in.
  public init(isPrivate: Bool, readable: Int, examples: [String], offered: Bool, signInCopies: Bool) {
    self.private = isPrivate
    self.readable = readable
    self.examples = examples
    self.offered = offered
    self.signInCopies = signInCopies
  }

  /// What `atlas onboarding files` said, or nil when it did not work or said something else.
  ///
  /// - Parameter outcome: The command's outcome.
  /// - Returns: The home folder's standing, or nil.
  public static func of(_ outcome: Outcome) -> HomeFiles? {
    guard outcome.ok else { return nil }
    return outcome.decode(HomeFiles.self)
  }
}
