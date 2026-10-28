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
    /// Whether it is live: not ended, and seen lately.
    public var isLive: Bool
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
      self.state = state
      self.waitingFor = waitingFor
      self.waitingSeconds = waitingSeconds
      self.holdsHelper = holdsHelper
      self.latest = latest
      self.uncertainRow = uncertainRow
      self.uncertainVerb = uncertainVerb
    }

    /// The fields of one `hostSessions` entry this app reads.
    enum CodingKeys: String, CodingKey {
      /// Each field by the name the service writes.
      case key, host, hostLabel, name, folder, started, lastSeen, ended, live, state, waitingFor, waitingSeconds,
        holdsHelper, latest, uncertainRow, uncertainVerb
    }

    /// Decodes a session, keeping the default of anything missing or of the wrong type. Times are read as
    /// milliseconds since 1970.
    ///
    /// - Parameter decoder: The decoder reading one `hostSessions` entry.
    /// - Throws: `DecodingError` when the entry is not an object or names no key or no name, which drops it.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      let text = { (key: CodingKeys) in
        (try? container.decodeIfPresent(String.self, forKey: key))
          .flatMap { $0.isEmpty ? nil : $0 }
      }
      let time = { (key: CodingKeys) in
        (try? container.decodeIfPresent(Double.self, forKey: key))
          .flatMap { $0.isFinite ? Date(timeIntervalSince1970: $0 / 1000) : nil }
      }
      let seconds = { (key: CodingKeys) in
        (try? container.decodeIfPresent(Double.self, forKey: key))
          .flatMap { $0.isFinite && $0 >= 0 ? $0 : nil }
      }
      // The Terminal session's key is empty, so only a missing key drops the entry.
      guard let key = try? container.decodeIfPresent(String.self, forKey: .key), let name = text(.name)
      else {
        throw DecodingError.dataCorrupted(
          DecodingError.Context(codingPath: decoder.codingPath, debugDescription: "A session names no key or name")
        )
      }
      self.key = key
      self.name = name
      host = SessionHost(word: text(.host))
      hostLabel = text(.hostLabel) ?? ""
      folder = text(.folder)
      startedAt = time(.started)
      lastSeenAt = time(.lastSeen)
      endedAt = time(.ended)
      state = HostSessionState(word: text(.state))
      isLive = (try? container.decodeIfPresent(Bool.self, forKey: .live)) ?? (endedAt == nil && state != .ended)
      waitingFor = text(.waitingFor)
        .flatMap(HostSessionWait.init(rawValue:))
      waitingSeconds = seconds(.waitingSeconds)
      holdsHelper = (try? container.decodeIfPresent(Bool.self, forKey: .holdsHelper)) == true
      latest = try? container.decodeIfPresent(Activity.self, forKey: .latest)
      uncertainRow = text(.uncertainRow)
      uncertainVerb = text(.uncertainVerb)
    }
  }
}
