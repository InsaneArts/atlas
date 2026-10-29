import Foundation

/// The colour a drawn app tile takes, chosen from its app's name so one app always draws the same tile: in every
/// window, at every launch, on every Mac.
public enum MonogramTint: Int, CaseIterable, Equatable, Sendable {
  /// Indigo.
  case indigo
  /// Blue.
  case blue
  /// Teal.
  case teal
  /// Green.
  case green
  /// Amber.
  case amber
  /// Coral.
  case coral
  /// Pink.
  case pink
  /// Graphite.
  case graphite

  /// 2166136261: FNV-1a's 32-bit offset basis.
  static let offsetBasis: UInt32 = 2_166_136_261
  /// 16777619: FNV-1a's 32-bit prime.
  static let prime: UInt32 = 16_777_619

  /// The tint of an app's name: FNV-1a over the lowercased, composed name's UTF-8, among the tints. Swift's own
  /// hashing is seeded anew in each process, so it would give an app another colour at each launch.
  ///
  /// - Parameter name: The app's name.
  public init(name: String) {
    let hash = Self.hash(
      name.precomposedStringWithCanonicalMapping
        .lowercased()
    )
    self = Self.allCases[Int(hash % UInt32(Self.allCases.count))]
  }

  /// FNV-1a's 32-bit hash of a text's UTF-8.
  ///
  /// - Parameter text: The text.
  /// - Returns: Its hash.
  static func hash(_ text: String) -> UInt32 {
    text.utf8.reduce(offsetBasis) { hash, byte in
      (hash ^ UInt32(byte)) &* prime
    }
  }
}
