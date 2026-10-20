extension ViewerStatus {
  /// What is happening in the helper, as the service sums it up.
  package struct Overview: Decodable, Equatable, Sendable {
    /// The helper's mode; unavailable when missing or unknown.
    package var mode = HelperMode.unavailable
    /// Why the helper is in its mode, in the service's words.
    package var reason = Words.connecting
    /// Whether Recover fixes what keeps the helper not ready: its input stopped or lost its display.
    package var recoverable = false
    /// The helper's short account name.
    package var account: String?
    /// The account's full name ("Atlas"), when it has one.
    package var accountName: String?
    /// The helper as people know it: its full name, else its short one.
    package var title: String? { accountName ?? account }
    /// The helper's display, in points.
    package var display: Size?
    /// Actions of unknown outcome, waiting for recovery.
    package var pending = 0
    /// Recent operations, newest first.
    package var activity: [Activity] = []

    /// Creates an overview of a helper not reached yet.
    package init() {}

    /// The fields of the `overview` object.
    enum CodingKeys: String, CodingKey {
      case mode, reason, recoverable, account, accountName, display, pending, activity
    }

    /// Decodes an overview, keeping the default of anything missing or of the wrong type.
    ///
    /// - Parameter decoder: The decoder reading the `overview` object.
    /// - Throws: `DecodingError` only when it is not an object.
    package init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      mode = (try? container.decodeIfPresent(HelperMode.self, forKey: .mode)) ?? .unavailable
      reason = (try? container.decodeIfPresent(String.self, forKey: .reason)) ?? reason
      recoverable = (try? container.decodeIfPresent(Bool.self, forKey: .recoverable)) ?? false
      account = try? container.decodeIfPresent(String.self, forKey: .account)
      accountName = try? container.decodeIfPresent(String.self, forKey: .accountName)
      display = try? container.decodeIfPresent(Size.self, forKey: .display)
      pending = (try? container.decodeIfPresent(Int.self, forKey: .pending)) ?? 0
      activity = (try? container.decodeIfPresent([Activity].self, forKey: .activity)) ?? []
    }
  }
}
