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
    /// Actions a live process is still carrying out, whose outcome is not known yet.
    package var inFlight = 0
    /// Recent operations, newest first.
    package var activity: [Activity] = []

    /// Whether Recover helps now: an action's outcome is unknown, or what keeps the helper not ready is something
    /// Recover fixes. The panel's recovery card and the viewer's alert both follow it.
    package var recoveryHelps: Bool {
      mode == .recovering || (mode == .blocked && recoverable)
    }

    /// Whether Recover restarts the helper, as the viewer service's own recovery decides: only while an action's
    /// outcome is unknown or under way; otherwise it starts the helper's input.
    package var recoveryRestarts: Bool {
      pending + inFlight > 0
    }

    /// Whether the command line's Recover may run now. Starting input or restarting takes the controller lock, which
    /// a viewer in control and an agent's work under way both hold, and checks the helper, which an unavailable or
    /// logged-out one fails; recovering is the recovery card's.
    package var recoverMayRun: Bool {
      switch mode {
      case .observing, .blocked: true
      case .agent: inFlight == 0
      case .controlling, .recovering, .unavailable, .loggedOut: false
      }
    }

    /// Creates an overview of a helper not reached yet.
    package init() {}

    /// The fields of the `overview` object.
    enum CodingKeys: String, CodingKey {
      case mode, reason, recoverable, account, accountName, display, pending, inFlight, activity
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
      inFlight = (try? container.decodeIfPresent(Int.self, forKey: .inFlight)) ?? 0
      activity = (try? container.decodeIfPresent([Activity].self, forKey: .activity)) ?? []
    }
  }
}
