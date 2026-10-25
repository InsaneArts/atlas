extension ViewerStatus {
  /// What is happening in the helper, as the service sums it up.
  public struct Overview: Decodable, Equatable, Sendable {
    /// The helper's mode; unavailable when missing or unknown. The rules (Stop, Resume, Recover) follow it.
    public var mode = HelperMode.unavailable
    /// What the helper is doing, as every agent app shows it; not ready when missing. The words follow it.
    public var state = OverviewState.notReady
    /// The state in the service's words, such as "Working"; empty when it gives none.
    public var label = ""
    /// Why the helper is in its mode, in the service's words; nil before the service gives one, which the app reads as
    /// connecting.
    public var reason: String?
    /// Whether Recover fixes what keeps the helper not ready: its input stopped or lost its display.
    public var recoverable = false
    /// The helper's short account name.
    public var account: String?
    /// The account's full name ("Atlas"), when it has one.
    public var accountName: String?
    /// The helper as people know it: its full name, else its short one.
    public var title: String? { accountName ?? account }
    /// The helper's display, in points.
    public var display: Size?
    /// Actions of unknown outcome, waiting for recovery.
    public var pending = 0
    /// Actions a live process is still carrying out, whose outcome is not known yet.
    var inFlight = 0
    /// Recent operations, newest first.
    public var activity: [Activity] = []

    /// How long after an agent's last action Stop Agent stays offered, in seconds: an agent between two steps still
    /// reads as working, so Stop stays where the person reaches for it.
    static let stopStaysFor: Double = 60

    /// Whether Stop Agent is offered: an agent acts, or acted within ``stopStaysFor``.
    public var offersStop: Bool {
      mode == .agent || (mode == .observing && agentActed(within: Self.stopStaysFor))
    }

    /// Whether Resume Agent is offered: the person stopped agents.
    public var offersResume: Bool { mode == .stopped }

    /// Whether an agent did something in the helper within `seconds`, so it may still be at work between steps.
    ///
    /// - Parameter seconds: How recent counts, in seconds.
    /// - Returns: Whether the newest agent row is that recent.
    func agentActed(within seconds: Double) -> Bool {
      guard let newest = activity.first(where: { $0.actor != "viewer" }) else { return false }
      return newest.ageSeconds.map { $0 < seconds } ?? false
    }

    /// Whether Recover helps now: an action's outcome is unknown, or what keeps the helper not ready is something
    /// Recover fixes. The panel's recovery card and the viewer's alert both follow it.
    public var recoveryHelps: Bool {
      mode == .recovering || (mode == .blocked && recoverable)
    }

    /// Whether Recover restarts the helper, as the viewer service's own recovery decides: only while an action's
    /// outcome is unknown or under way; otherwise it starts the helper's input.
    public var recoveryRestarts: Bool {
      pending > 0 || inFlight > 0
    }

    /// Whether the command line's Recover may run now. Starting input or restarting takes the controller lock, which
    /// a viewer in control and an agent's work under way both hold, and checks the helper, which an unavailable or
    /// logged-out one fails; recovering is the recovery card's.
    var recoverMayRun: Bool {
      switch mode {
      case .observing, .blocked: true
      case .agent, .stopped: inFlight == 0
      case .controlling, .recovering, .unavailable, .loggedOut: false
      }
    }

    /// Creates an overview of a helper not reached yet.
    public init() {}

    /// The fields of the `overview` object.
    enum CodingKeys: String, CodingKey {
      /// Each field by the name the service writes.
      case mode, state, label, reason, recoverable, account, accountName, display, pending, inFlight, activity
    }

    /// Decodes an overview, keeping the default of anything missing or of the wrong type.
    ///
    /// - Parameter decoder: The decoder reading the `overview` object.
    /// - Throws: `DecodingError` only when it is not an object.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      mode = (try? container.decodeIfPresent(HelperMode.self, forKey: .mode)) ?? .unavailable
      state = (try? container.decodeIfPresent(OverviewState.self, forKey: .state)) ?? .notReady
      label = (try? container.decodeIfPresent(String.self, forKey: .label)) ?? ""
      reason = try? container.decodeIfPresent(String.self, forKey: .reason)
      recoverable = (try? container.decodeIfPresent(Bool.self, forKey: .recoverable)) ?? false
      account = try? container.decodeIfPresent(String.self, forKey: .account)
      accountName = try? container.decodeIfPresent(String.self, forKey: .accountName)
      display = try? container.decodeIfPresent(Size.self, forKey: .display)
      pending = max(0, (try? container.decodeIfPresent(Int.self, forKey: .pending)) ?? 0)
      inFlight = max(0, (try? container.decodeIfPresent(Int.self, forKey: .inFlight)) ?? 0)
      activity = (try? container.decodeIfPresent([Activity].self, forKey: .activity)) ?? []
    }
  }
}
