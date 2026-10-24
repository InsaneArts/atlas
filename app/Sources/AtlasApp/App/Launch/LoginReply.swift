import Foundation

/// What `--login` answers on standard output, the protocol `atlas menubar --login` reads: `{"ok": true, "enabled":
/// …, "available": …}`, or `{"ok": false, "error": "…"}`.
struct LoginReply: Encodable, Equatable {
  /// Whether the change, or the report, worked.
  let ok: Bool
  /// Whether Atlas opens at login now; only when it worked.
  let enabled: Bool?
  /// Whether this app can open at login at all; only when it worked.
  let available: Bool?
  /// Why it failed; only when it did.
  let error: String?

  /// Where Open at Login stands now.
  ///
  /// - Parameter state: What the login item reports.
  /// - Returns: The reply.
  static func state(_ state: OpenAtLogin.State) -> LoginReply {
    LoginReply(ok: true, enabled: state.isEnabled, available: state.isAvailable, error: nil)
  }

  /// Why the change failed.
  ///
  /// - Parameter message: The system's reason, in its own words.
  /// - Returns: The reply.
  static func failure(_ message: String) -> LoginReply {
    LoginReply(ok: false, enabled: nil, available: nil, error: message)
  }

  /// The reply as one line of JSON, escaped by the encoder, so a reason with quotes, backslashes or line breaks
  /// stays valid; a missing field is left out.
  var json: Data {
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
    // Only Bools and a String: encoding cannot fail.
    return (try? encoder.encode(self)) ?? Data(#"{"ok":false}"#.utf8)
  }
}
