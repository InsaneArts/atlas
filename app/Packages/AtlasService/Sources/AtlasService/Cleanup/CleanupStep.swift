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
  /// Why it was kept, failed or did not run; nil for none.
  public var reason: Reason?
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
  /// The id of the background job a stop-job step stops; nil for every other kind.
  public var job: String?
  /// The name of that job, as the agent gave it or its program's name; nil for every other kind.
  public var name: String?
  /// The processes that left the stopped job's group and still ran, which Atlas never stopped.
  public var left: [CleanupProcess]
  /// How many files' collected copies are missing or changed, which cleanup keeps.
  public var notKept: Int

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
  ///   - job: The id of the job a stop-job step stops; none by default.
  ///   - name: That job's name; none by default.
  ///   - left: The processes that left the job's group and still ran; none by default.
  ///   - notKept: How many files' collected copies are missing or changed; none by default.
  public init(
    number: Int,
    kind: CleanupStepKind,
    apps: [String] = [],
    app: String? = nil,
    state: CleanupStepState? = nil,
    reason: Reason? = nil,
    retry: Bool = false,
    alreadyGone: Bool = false,
    held: String? = nil,
    unchanged: Bool? = nil,
    job: String? = nil,
    name: String? = nil,
    left: [CleanupProcess] = [],
    notKept: Int = 0
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
    self.job = job
    self.name = name
    self.left = left
    self.notKept = notKept
  }

  /// The fields of one `steps` entry this app reads.
  enum CodingKeys: String, CodingKey {
    /// Its place, which the service writes as `n`.
    case number = "n"
    /// What it does, which the service writes as `do`.
    case kind = "do"
    /// Each other field by the name the service writes.
    case apps, app, state, reason, retry, alreadyGone, held, unchanged, name, left, notKept
    /// The job a stop-job step stops, which the service writes as `id`.
    case job = "id"
  }

  /// Decodes a step, keeping the default of anything missing or of the wrong type; a kind or reason this app does not
  /// know reads as unknown, and each app a quit step names, and each process, decodes on its own. Every file not kept
  /// counts, however odd its entry.
  ///
  /// - Parameter decoder: The decoder reading one `steps` entry.
  /// - Throws: `DecodingError` only when the entry is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    number = container.lenientCount(.number)
    kind = CleanupStepKind(word: container.lenientText(.kind))
    apps = container.lenientAppNames(.apps)
    app = container.lenientText(.app)
    state = CleanupStepState(word: container.lenientText(.state))
    reason = Reason(word: container.lenientText(.reason))
    retry = container.lenientFlag(.retry)
    alreadyGone = container.lenientFlag(.alreadyGone)
    held = container.lenientText(.held)
    unchanged = container.lenientBool(.unchanged)
    job = container.lenientText(.job)
    name = container.lenientText(.name)
    left = container.lenientList(CleanupProcess.self, forKey: .left)
    notKept = container.lenientList(LenientEntry.self, forKey: .notKept).count
  }
}
