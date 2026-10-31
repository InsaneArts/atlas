import Foundation

extension ViewerStatus {
  /// One host session the overview lists: an agent's chat or run in a host such as Claude Code, or the commands no
  /// host named, gathered as Terminal. A label for the person's eyes: nothing reads it to decide who may act.
  public struct HostSession: Decodable, Equatable, Sendable, Identifiable {
    /// The session's key, which holds as its state changes; empty for Terminal.
    public var key: String
    /// The app it runs in.
    public var host: SessionHost
    /// The host's name in the service's words, such as "Claude Code"; empty when it gives none.
    public var hostLabel: String
    /// The session's name, which Atlas makes unique among those listed, such as "Claude Code · understudy".
    public var name: String
    /// The project folder's name, never a path; nil when unknown.
    public var folder: String?
    /// When it started.
    public var startedAt: Date?
    /// When it last did something.
    public var lastSeenAt: Date?
    /// When it ended; nil while it has not.
    public var endedAt: Date?
    /// Whether its host or proven command is live, independently of recovery attention.
    public var isLive: Bool
    /// Whether a verified command still works, independently of the host's lifetime.
    public var workInProgress: Bool
    /// What it is doing.
    public var state: HostSessionState
    /// What it waits for, while it waits.
    public var waitingFor: HostSessionWait?
    /// How long it has waited for the helper, in seconds, while it does.
    public var waitingSeconds: Double?
    /// Whether it has the helper now.
    public var holdsHelper: Bool
    /// Its newest action, when the service read one.
    public var latest: Activity?
    /// The row id of its newest action whose outcome is unknown, while the helper needs the person.
    public var uncertainRow: String?
    /// That action's verb, such as `click` or `type`.
    public var uncertainVerb: String?

    /// The session's identity: its key.
    public var id: String { key }

    /// Creates a session.
    ///
    /// - Parameters:
    ///   - key: Its key; empty for Terminal.
    ///   - name: Its name.
    ///   - host: The app it runs in.
    ///   - hostLabel: The host's name in the service's words.
    ///   - folder: The project folder's name.
    ///   - startedAt: When it started.
    ///   - lastSeenAt: When it last did something.
    ///   - endedAt: When it ended.
    ///   - isLive: Whether it is live; live by default.
    ///   - workInProgress: Whether a verified command still works.
    ///   - state: What it is doing; idle by default.
    ///   - waitingFor: What it waits for.
    ///   - waitingSeconds: How long it has waited for the helper, in seconds.
    ///   - holdsHelper: Whether it has the helper now.
    ///   - latest: Its newest action.
    ///   - uncertainRow: The row id of its newest action whose outcome is unknown.
    ///   - uncertainVerb: That action's verb.
    public init(
      key: String,
      name: String,
      host: SessionHost,
      hostLabel: String = "",
      folder: String? = nil,
      startedAt: Date? = nil,
      lastSeenAt: Date? = nil,
      endedAt: Date? = nil,
      isLive: Bool = true,
      workInProgress: Bool = false,
      state: HostSessionState = .idle,
      waitingFor: HostSessionWait? = nil,
      waitingSeconds: Double? = nil,
      holdsHelper: Bool = false,
      latest: Activity? = nil,
      uncertainRow: String? = nil,
      uncertainVerb: String? = nil
    ) {
      self.key = key
      self.name = name
      self.host = host
      self.hostLabel = hostLabel
      self.folder = folder
      self.startedAt = startedAt
      self.lastSeenAt = lastSeenAt
      self.endedAt = endedAt
      self.isLive = isLive
      self.workInProgress = workInProgress
      self.state = state
      self.waitingFor = waitingFor
      self.waitingSeconds = waitingSeconds
      self.holdsHelper = holdsHelper
      self.latest = latest
      self.uncertainRow = uncertainRow
      self.uncertainVerb = uncertainVerb
      normalizeLifetime()
    }

    /// The fields of one `hostSessions` entry this app reads.
    enum CodingKeys: String, CodingKey {
      /// Each field by the name the service writes.
      case key, host, hostLabel, name, folder, started, lastSeen, ended, live, state, waitingFor, waitingSeconds,
        holdsHelper, latest, uncertainRow, uncertainVerb, workInProgress
    }

    /// Decodes a session, keeping the default of anything missing or of the wrong type. Times are read as
    /// milliseconds since 1970.
    ///
    /// - Parameter decoder: The decoder reading one `hostSessions` entry.
    /// - Throws: `DecodingError` when the entry is not an object or names no key or no name, which drops it.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      // The Terminal session's key is empty, so only a missing key drops the entry.
      guard let key = try? container.decodeIfPresent(String.self, forKey: .key),
        let name = container.lenientText(.name)
      else {
        throw DecodingError.dataCorrupted(
          DecodingError.Context(codingPath: decoder.codingPath, debugDescription: "A session names no key or name")
        )
      }
      self.key = key
      self.name = name
      host = SessionHost(word: container.lenientText(.host))
      hostLabel = container.lenientText(.hostLabel) ?? ""
      folder = container.lenientText(.folder)
      startedAt = container.lenientDate(milliseconds: .started)
      lastSeenAt = container.lenientDate(milliseconds: .lastSeen)
      endedAt = container.lenientDate(milliseconds: .ended)
      state = HostSessionState(word: container.lenientText(.state))
      workInProgress = container.lenientFlag(.workInProgress)
      isLive = container.lenientBool(.live) ?? (endedAt == nil && state != .ended)
      waitingFor = container.lenientText(.waitingFor)
        .flatMap(HostSessionWait.init(rawValue:))
      waitingSeconds = (try? container.decodeIfPresent(Double.self, forKey: .waitingSeconds))
        .flatMap { $0.isFinite && $0 >= 0 ? $0 : nil }
      holdsHelper = container.lenientFlag(.holdsHelper)
      latest = try? container.decodeIfPresent(Activity.self, forKey: .latest)
      uncertainRow = container.lenientText(.uncertainRow)
      uncertainVerb = container.lenientText(.uncertainVerb)
      normalizeLifetime()
    }
    /// Normalize older replies using only service facts, never local process guesses.
    private mutating func normalizeLifetime() {
      if endedAt != nil || state == .ended {
        isLive = false
        if state != .needsYou && !(workInProgress && (state == .acting || state == .waiting)) {
          state = .ended
          waitingFor = nil
          waitingSeconds = nil
        }
      }
      if !isLive { holdsHelper = false }
    }

  }
}
