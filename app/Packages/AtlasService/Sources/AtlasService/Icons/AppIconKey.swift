import Foundation

/// What an app's icon is known by: its bundle identifier, such as `com.apple.TextEdit`, else its name. Neither is ever
/// a path, so an icon kept by its key names no folder, document or person.
public enum AppIconKey: Hashable, Sendable {
  /// A bundle identifier: 1 to 155 letters, digits, dots and hyphens, with no empty part, as the service keeps one.
  case bundle(String)
  /// An app's name, trimmed, lowercased and composed, so "Notes" and "notes" are one app.
  case name(String)

  /// 155: the longest bundle identifier, as the service keeps one.
  public static let longestBundleId = 155
  /// 40: the longest name, as the service keeps an app's name.
  public static let longestName = 40
  /// What the service ends a name it cut short with; a cut name is no app's whole name, so it is never a key.
  static let cutMark: Character = "…"

  /// The key of a bundle identifier, when the text is one.
  ///
  /// - Parameter bundleId: The text, such as an Activity row's or an app's `bundleId`.
  public init?(bundleId: String?) {
    guard let bundleId, Self.isBundleId(bundleId) else { return nil }
    self = .bundle(bundleId)
  }

  /// The key of an app's name, when it can be one: not empty, not past ``longestName``, and nothing a path or a cut
  /// name holds (a slash, `..` or a trailing `…`).
  ///
  /// - Parameter name: The app's name, as the service gives it.
  public init?(name: String?) {
    guard let name else { return nil }
    let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty, trimmed.count <= Self.longestName, !trimmed.contains("/"), !trimmed.contains(".."),
      trimmed.last != Self.cutMark
    else { return nil }
    self = .name(trimmed.precomposedStringWithCanonicalMapping.lowercased())
  }

  /// The text the key is kept under, which a bundle identifier and a name never share.
  public var text: String {
    switch self {
    case .bundle(let id): "bundle:\(id)"
    case .name(let name): "name:\(name)"
    }
  }

  /// The bundle identifier, for a key that is one.
  public var bundleId: String? {
    guard case .bundle(let id) = self else { return nil }
    return id
  }

  /// Whether the text is a bundle identifier: 1 to ``longestBundleId`` ASCII letters, digits, dots and hyphens, with
  /// no empty part, so free text or a path never is.
  ///
  /// - Parameter text: The text.
  /// - Returns: Whether it is one.
  public static func isBundleId(_ text: String) -> Bool {
    let characters = text.utf8
    return !characters.isEmpty && characters.count <= longestBundleId
      && characters.allSatisfy { byte in
        (byte >= UInt8(ascii: "a") && byte <= UInt8(ascii: "z"))
          || (byte >= UInt8(ascii: "A") && byte <= UInt8(ascii: "Z"))
          || (byte >= UInt8(ascii: "0") && byte <= UInt8(ascii: "9"))
          || byte == UInt8(ascii: ".") || byte == UInt8(ascii: "-")
      }
      && text.split(separator: ".", omittingEmptySubsequences: false)
        .allSatisfy { !$0.isEmpty }
  }
}
