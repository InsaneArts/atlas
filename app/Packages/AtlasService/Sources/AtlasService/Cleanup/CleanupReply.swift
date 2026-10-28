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
    /// One app the cleanup leaves running: its name, beside why, which this app does not show.
    struct Left: Decodable {
      /// The app's name.
      let app: String?
    }
    let container = try decoder.container(keyedBy: CodingKeys.self)
    let flag = { (key: CodingKeys) in
      (try? container.decodeIfPresent(Bool.self, forKey: key)) == true
    }
    ok = flag(.ok)
    preview = flag(.preview)
    state = CleanupState(word: try? container.decodeIfPresent(String.self, forKey: .state))
    steps =
      (try? container.decodeIfPresent([Lenient<CleanupStep>].self, forKey: .steps))?
      .compactMap(\.value) ?? []
    left =
      (try? container.decodeIfPresent([Lenient<Left>].self, forKey: .left))?
      .compactMap { $0.value?.app }
      .filter { !$0.isEmpty } ?? []
    active = flag(.active)
    error =
      (try? container.decodeIfPresent(String.self, forKey: .error))
      .flatMap { $0.isEmpty ? nil : $0 }
  }
}
