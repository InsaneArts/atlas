/// What was done in the helper lately, as the viewer service's `activity.json` lists it: up to 200 rows, newest
/// first, with no typed text, commands, coordinates or window titles. Every field may be missing, so each has a safe
/// default.
package struct ActivityLog: Decodable, Equatable, Sendable {
  /// The rows, newest first. A command under way has a row of its own, replaced by a new row with a new id when it
  /// ends.
  package var entries: [Entry] = []

  /// Creates a log of `entries`.
  ///
  /// - Parameter entries: The rows, newest first.
  package init(entries: [Entry] = []) {
    self.entries = entries
  }

  /// The root fields of `activity.json`.
  enum CodingKeys: String, CodingKey {
    case entries = "activity"
  }

  /// Decodes a log; rows that are missing or of the wrong type read as none.
  ///
  /// - Parameter decoder: The decoder reading `activity.json`.
  /// - Throws: `DecodingError` only when the reply is not an object.
  package init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    entries = (try? container.decodeIfPresent([Entry].self, forKey: .entries)) ?? []
  }
}
