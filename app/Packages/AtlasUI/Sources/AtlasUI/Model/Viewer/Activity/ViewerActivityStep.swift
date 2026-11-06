/// One step of an open moment: its disc on the moment's line, what was done, when, the screenshot it took, the
/// marked picture of the screen just before it, and for one that did not work, a callout that says what happened, why
/// and what came next.
public struct ViewerActivityStep: Equatable, Sendable, Identifiable {
  /// The step's identity: its earliest row's id, never reused.
  public var id: String
  /// What kind of thing it did, which its disc's glyph shows.
  public var kind: ViewerActivityKind
  /// How it went, which its disc's colour shows.
  public var mark: ViewerStepMark
  /// What was done; inside the callout for a step that did not work.
  public var words: ViewerStepWords
  /// When it was done, such as "now", "4 s", "3 min" or "10:31".
  public var time: String
  /// The screenshot it took; nil for every step but a screenshot's.
  public var shot: ViewerActivityShot?
  /// The marked picture of the screen taken right before it; nil for every step but an agent's input that kept one.
  public var picture: ViewerStepPicture?
  /// Why it did not work and what came next; nil for a step that worked or runs.
  public var problem: ViewerActivityProblem?
  /// Whether it is the action whose outcome is unknown while the helper needs the person: its callout offers Show
  /// Screen and Recover….
  public var offersRecovery: Bool
  /// Whether it typed a password, whose text Atlas never keeps, which a lock badge says.
  public var notKept: Bool
  /// Whether it is its moment's last step shown, where the moment's line ends.
  public var isLast: Bool

  /// Creates a step.
  ///
  /// - Parameters:
  ///   - id: Its earliest row's id.
  ///   - kind: What kind of thing it did.
  ///   - mark: How it went; quiet by default.
  ///   - words: What was done.
  ///   - time: When it was done.
  ///   - shot: The screenshot it took; none by default.
  ///   - picture: The marked picture taken right before it; none by default.
  ///   - problem: Why it did not work; none by default.
  ///   - offersRecovery: Whether it offers recovery; it does not by default.
  ///   - notKept: Whether it typed a password Atlas did not keep; it did not by default.
  ///   - isLast: Whether it is its moment's last step shown; it is not by default.
  public init(
    id: String,
    kind: ViewerActivityKind,
    mark: ViewerStepMark = .quiet,
    words: ViewerStepWords,
    time: String,
    shot: ViewerActivityShot? = nil,
    picture: ViewerStepPicture? = nil,
    problem: ViewerActivityProblem? = nil,
    offersRecovery: Bool = false,
    notKept: Bool = false,
    isLast: Bool = false
  ) {
    self.id = id
    self.kind = kind
    self.mark = mark
    self.words = words
    self.time = time
    self.shot = shot
    self.picture = picture
    self.problem = problem
    self.offersRecovery = offersRecovery
    self.notKept = notKept
    self.isLast = isLast
  }

  /// What VoiceOver reads for it: "Clicked Save, 4 s", with why it did not work and what came next, or ", now" while
  /// it runs; a password's typing says it was not kept.
  public var label: String {
    let parts = [words.text, notKept ? "Not kept" : "", time, problem?.why ?? "", problem?.next ?? ""]
    return parts.filter { !$0.isEmpty }
      .joined(separator: ", ")
  }
}
