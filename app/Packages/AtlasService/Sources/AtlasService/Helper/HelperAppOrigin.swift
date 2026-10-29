import Foundation

/// Which agent's session opened an app open in the helper, how, and when, as `apps.json` says; it holds a session's
/// key and a time, never a path.
public struct HelperAppOrigin: Decodable, Equatable, Sendable {
  /// The key of the host session whose agent opened it; empty for an agent no host named.
  public var session: String
  /// How the session came to open it.
  public var how: How
  /// When it was opened, when known.
  public var at: Date?

  /// Creates an origin.
  ///
  /// - Parameters:
  ///   - session: The key of the host session whose agent opened it.
  ///   - how: How the session came to open it.
  ///   - at: When it was opened.
  public init(session: String, how: How, at: Date? = nil) {
    self.session = session
    self.how = how
    self.at = at
  }

  /// The fields of one app's `origin`.
  enum CodingKeys: String, CodingKey {
    /// Each field by the name the service writes.
    case session, how, at
  }

  /// Decodes an origin, keeping a safe default for anything missing or of the wrong type: no session is Terminal's
  /// empty key, and a way this app does not know reads as opened by name.
  ///
  /// - Parameter decoder: The decoder reading one app's `origin`.
  /// - Throws: `DecodingError` only when the origin is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    session = (try? container.decodeIfPresent(String.self, forKey: .session)) ?? ""
    how = How(word: container.lenientText(.how))
    at = container.lenientDate(milliseconds: .at)
  }
}
