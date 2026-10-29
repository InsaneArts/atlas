import Foundation

/// One host session as `sessions.json` lists it: who it is, when it ran, and what it made in the helper. It carries no
/// host id, path, plan id or copy token.
public struct HostSessionRecord: Decodable, Equatable, Sendable, Identifiable {
  /// The session's key; empty for Terminal.
  public var key: String
  /// The app it runs in.
  public var host: SessionHost
  /// The host's name in the service's words, such as "Pi"; empty when it gives none.
  public var hostLabel: String
  /// The session's name, such as "Pi · notes-sync".
  public var name: String
  /// The project folder's name, never a path; nil when unknown.
  public var folder: String?
  /// When it started.
  public var startedAt: Date?
  /// When it last did something.
  public var lastSeenAt: Date?
  /// When it ended; nil while it has not.
  public var endedAt: Date?
  /// Ended, or idle for a live one: a live session's state comes from the status, which is always fresher.
  public var state: HostSessionState
  /// How many actions it took today.
  public var actions: Int
  /// The apps it opened.
  public var apps: [HostSessionApp]
  /// The copies of apps Atlas made for it.
  public var copies: [HostSessionCopy]
  /// The environment plans it applied.
  public var plans: [HostSessionPlan]
  /// The names of the apps its tasks installed that may still be in the helper's Applications.
  public var installs: [String]
  /// Where its newest cleanup that did not finish stands; nil when every cleanup finished, or none ran.
  public var cleanup: CleanupState?
  /// Its newest screenshots' file names, newest first.
  public var screenshots: [String]

  /// The session's identity: its key.
  public var id: String { key }

  /// Whether the list says it ended: its state, or the time it ended.
  public var isEnded: Bool { state == .ended || endedAt != nil }

  /// Whether it left something to clean up: a copy, a plan, an installed app, or a cleanup that did not finish.
  public var leftWork: Bool {
    let unfinished = cleanup.map { $0 != .complete } == true
    return !copies.isEmpty || !plans.isEmpty || !installs.isEmpty || unfinished
  }

  /// Creates a record.
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
  ///   - state: Ended, or idle for a live one.
  ///   - actions: How many actions it took today.
  ///   - apps: The apps it opened.
  ///   - copies: The copies of apps made for it.
  ///   - plans: The environment plans it applied.
  ///   - installs: The apps its tasks installed that may still be there.
  ///   - cleanup: Where its newest unfinished cleanup stands.
  ///   - screenshots: Its newest screenshots' file names.
  public init(
    key: String,
    name: String,
    host: SessionHost,
    hostLabel: String = "",
    folder: String? = nil,
    startedAt: Date? = nil,
    lastSeenAt: Date? = nil,
    endedAt: Date? = nil,
    state: HostSessionState = .idle,
    actions: Int = 0,
    apps: [HostSessionApp] = [],
    copies: [HostSessionCopy] = [],
    plans: [HostSessionPlan] = [],
    installs: [String] = [],
    cleanup: CleanupState? = nil,
    screenshots: [String] = []
  ) {
    self.key = key
    self.name = name
    self.host = host
    self.hostLabel = hostLabel
    self.folder = folder
    self.startedAt = startedAt
    self.lastSeenAt = lastSeenAt
    self.endedAt = endedAt
    self.state = state
    self.actions = actions
    self.apps = apps
    self.copies = copies
    self.plans = plans
    self.installs = installs
    self.cleanup = cleanup
    self.screenshots = screenshots
  }

  /// The fields of one `sessions` entry.
  enum CodingKeys: String, CodingKey {
    /// Each field by the name the service writes.
    case key, host, hostLabel, name, folder, startedAt, lastSeenAt, endedAt, state, actions, apps, copies, plans,
      installs, cleanup, screenshots
  }

  /// Decodes a record, keeping the default of anything missing or of the wrong type; each app, copy, plan and install
  /// decodes on its own. Times are read as milliseconds since 1970.
  ///
  /// - Parameter decoder: The decoder reading one `sessions` entry.
  /// - Throws: `DecodingError` when the entry is not an object or names no key or no name, which drops it.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    // The Terminal session's key is empty, so only a missing key drops the entry.
    guard let key = try? container.decodeIfPresent(String.self, forKey: .key), let name = container.lenientText(.name)
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
    startedAt = container.lenientDate(milliseconds: .startedAt)
    lastSeenAt = container.lenientDate(milliseconds: .lastSeenAt)
    endedAt = container.lenientDate(milliseconds: .endedAt)
    state = HostSessionState(word: container.lenientText(.state))
    actions = container.lenientCount(.actions)
    apps = container.lenientList(HostSessionApp.self, forKey: .apps)
    copies = container.lenientList(HostSessionCopy.self, forKey: .copies)
    plans = container.lenientList(HostSessionPlan.self, forKey: .plans)
    installs = container.lenientList(String.self, forKey: .installs)
      .filter { !$0.isEmpty }
    cleanup = CleanupState(word: container.lenientText(.cleanup))
    screenshots = container.lenientList(String.self, forKey: .screenshots)
      .filter { !$0.isEmpty }
  }
}
