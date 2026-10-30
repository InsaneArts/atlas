/// What cleaning up a session's work takes or took: `cleanup.json`'s preview, or the reply to `cleanup`'s apply, which
/// still lists its steps when it did not finish. Every field may be missing, so each has a safe default.
public struct CleanupReply: Decodable, Equatable, Sendable {
  /// Whether the cleanup is done: for an apply, every step done or kept on purpose; for a preview, that it was read.
  public var ok: Bool
  /// Whether this is a preview, which changed nothing, rather than an apply.
  public var preview: Bool
  /// How far an apply got; nil for a preview, which runs nothing.
  public var state: CleanupState?
  /// The steps it takes, in order: none when there is nothing to clean up.
  public var steps: [CleanupStep]
  /// The apps the session opened by name, which the cleanup leaves running; none when the service names none.
  public var left: [String]
  /// Whether the session may still be working: it acted in the last few minutes, or its host still runs.
  public var active: Bool
  /// Why an apply did not finish, in the service's words; nil when it did, or the service gave none.
  public var error: String?

  /// Creates a reply.
  ///
  /// - Parameters:
  ///   - ok: Whether the cleanup is done, or the preview was read; not by default.
  ///   - preview: Whether this is a preview; not by default.
  ///   - state: How far an apply got; none by default.
  ///   - steps: The steps, in order; none by default.
  ///   - left: The apps it leaves running; none by default.
  ///   - active: Whether the session may still be working; not by default.
  ///   - error: Why an apply did not finish; none by default.
  public init(
    ok: Bool = false,
    preview: Bool = false,
    state: CleanupState? = nil,
    steps: [CleanupStep] = [],
    left: [String] = [],
    active: Bool = false,
    error: String? = nil
  ) {
    self.ok = ok
    self.preview = preview
    self.state = state
    self.steps = steps
    self.left = left
    self.active = active
    self.error = error
  }

  /// Whether there is nothing to clean up: no step at all.
  public var isEmpty: Bool { steps.isEmpty }

  /// Whether a step started and has not ended, as while a cleanup runs, or after one was cut short mid-step.
  public var isUnderWay: Bool { steps.contains { $0.state == .started } }

  /// Whether any step has run, so the reply says how far a cleanup got.
  public var hasRun: Bool { steps.contains { $0.state != nil } }

  /// The steps still to do: every one but those a cleanup that did not finish before already did, or kept.
  public var openSteps: [CleanupStep] {
    steps.filter { $0.state != .done && $0.state != .kept }
  }

  /// How an apply that replied ended: not knowing whether its last step finished and being held back each read on
  /// their own; else cleaned up, or nothing to clean up, only when it says it is done.
  public var ending: CleanupEnding {
    switch state {
    case .uncertain: return .uncertain
    case .blocked: return .blocked(heldBackBy)
    case .complete, .incomplete, .unknown, nil: break
    }
    guard ok, state == nil || state == .complete else { return .unfinished }
    return isEmpty ? .nothingToCleanUp : .cleanedUp
  }

  /// How a cleanup whose reply never came ended, read from `cleanup.json` once it shows no step under way: the steps
  /// a run left read as an apply's do, and with nothing left it cleaned up. Nil when no step has run, as nothing is
  /// known of how it went.
  public var unansweredEnding: CleanupEnding? {
    guard !isEmpty else { return .cleanedUp }
    let states = steps.compactMap(\.state)
    guard !states.isEmpty else { return nil }
    if states.contains(where: { $0 == .started || $0 == .uncertain }) { return .uncertain }
    return states.contains(.blocked) ? .blocked(heldBackBy) : .unfinished
  }

  /// Why the steps held back were held back: a collected copy no longer kept wins, then a helper that could not be
  /// reached, then whatever the first held step gives.
  private var heldBackBy: CleanupStep.Reason? {
    let reasons = steps.filter { $0.state == .blocked }
      .map(\.reason)
    if reasons.contains(.outputNotKept) { return .outputNotKept }
    if reasons.contains(.daemon) { return .daemon }
    return reasons.first ?? nil
  }

  /// The body's fields this app reads.
  enum CodingKeys: String, CodingKey {
    /// Each field by the name the service writes.
    case ok, preview, state, steps, left, active, error
  }

  /// Decodes the body, keeping the default of anything missing or of the wrong type; each step decodes on its own, so
  /// one odd step is dropped and the rest are kept.
  ///
  /// - Parameter decoder: The decoder reading the body.
  /// - Throws: `DecodingError` only when the body is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    ok = container.lenientFlag(.ok)
    preview = container.lenientFlag(.preview)
    state = CleanupState(word: container.lenientText(.state))
    steps = container.lenientList(CleanupStep.self, forKey: .steps)
    left = container.lenientAppNames(.left)
    active = container.lenientFlag(.active)
    error = container.lenientText(.error)
  }
}
