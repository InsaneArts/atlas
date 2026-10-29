/// One step of cleaning up a session's work, as `cleanup.json` and `cleanup` list it: what it does, to which app, and,
/// once it has run, how far it got and why. It carries no path, plan id or install id.
public struct CleanupStep: Decodable, Equatable, Sendable {
  /// Its place among the steps, from 1; 0 when the service gives none.
  public var number: Int
  /// What it does.
  public var kind: CleanupStepKind
  /// The apps a quit step quits, by name, in the service's order; none for every other kind.
  public var apps: [String]
  /// The app an install or a copy belongs to; nil when the service names none.
  public var app: String?
  /// How far it got; nil for a step that has not run.
  public var state: CleanupStepState?
  /// Why it was kept, failed or did not run, as the service's word, such as `changed` or `in-use`; nil for none.
  public var reason: String?
  /// Whether running the cleanup again may get it done.
  public var retry: Bool
  /// Whether what it takes away was gone before it ran.
  public var alreadyGone: Bool
  /// Where in the helper an installed app was moved aside to once the step held it, such as
  /// "/Users/atlas/.config/atlas/installs/held/a7d2c0ffee12/Probe.app"; nil before it ran, when it was already gone,
  /// or for any other step.
  public var held: String?
  /// Whether an installed app is as it was installed, so the step moves it aside in the helper; false when it changed
  /// and is kept, nil when the service does not say.
  public var unchanged: Bool?

  /// Creates a step.
  ///
  /// - Parameters:
  ///   - number: Its place among the steps, from 1.
  ///   - kind: What it does.
  ///   - apps: The apps a quit step quits; none by default.
  ///   - app: The app an install or a copy belongs to; none by default.
  ///   - state: How far it got; nil, not run, by default.
  ///   - reason: Why it was kept, failed or did not run; none by default.
  ///   - retry: Whether running again may get it done; not by default.
  ///   - alreadyGone: Whether what it takes away was gone before it ran; not by default.
  ///   - held: Where in the helper it was moved aside to; none by default.
  ///   - unchanged: Whether an installed app is as it was installed; unknown by default.
  public init(
    number: Int,
    kind: CleanupStepKind,
    apps: [String] = [],
    app: String? = nil,
    state: CleanupStepState? = nil,
    reason: String? = nil,
    retry: Bool = false,
    alreadyGone: Bool = false,
    held: String? = nil,
    unchanged: Bool? = nil
  ) {
    self.number = number
    self.kind = kind
    self.apps = apps
    self.app = app
    self.state = state
    self.reason = reason
    self.retry = retry
    self.alreadyGone = alreadyGone
    self.held = held
    self.unchanged = unchanged
  }

  /// The fields of one `steps` entry this app reads.
  enum CodingKeys: String, CodingKey {
    /// Its place, which the service writes as `n`.
    case number = "n"
    /// What it does, which the service writes as `do`.
    case kind = "do"
    /// Each other field by the name the service writes.
    case apps, app, state, reason, retry, alreadyGone, held, unchanged
  }

  /// Decodes a step, keeping the default of anything missing or of the wrong type; a kind this app does not know
  /// reads as ``CleanupStepKind/unknown``, and each app a quit step names decodes on its own.
  ///
  /// - Parameter decoder: The decoder reading one `steps` entry.
  /// - Throws: `DecodingError` only when the entry is not an object.
  public init(from decoder: any Decoder) throws {
    /// One app a quit step names: its name, beside a process id this app does not read.
    struct Quit: Decodable {
      /// The app's name.
      let app: String?
    }
    let container = try decoder.container(keyedBy: CodingKeys.self)
    let text = { (key: CodingKeys) in
      (try? container.decodeIfPresent(String.self, forKey: key))
        .flatMap { $0.isEmpty ? nil : $0 }
    }
    let flag = { (key: CodingKeys) in
      try? container.decodeIfPresent(Bool.self, forKey: key)
    }
    number = max(0, (try? container.decodeIfPresent(Int.self, forKey: .number)) ?? 0)
    kind = CleanupStepKind(word: text(.kind))
    apps =
      (try? container.decodeIfPresent([Lenient<Quit>].self, forKey: .apps))?
      .compactMap { $0.value?.app }
      .filter { !$0.isEmpty } ?? []
    app = text(.app)
    state = CleanupStepState(word: text(.state))
    reason = text(.reason)
    retry = flag(.retry) == true
    alreadyGone = flag(.alreadyGone) == true
    held = text(.held)
    unchanged = flag(.unchanged)
  }
}
