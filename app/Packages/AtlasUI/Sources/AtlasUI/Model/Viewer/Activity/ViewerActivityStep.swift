/// One step of an opened run, or a folded run's problem line: its glyph, what was done on one line, a picture button
/// when it kept a picture of the screen just before it, and its time when the minute changed. A screenshot's step
/// shows its picture in place under its line; a step that did not work says so in orange with why.
public struct ViewerActivityStep: Equatable, Sendable, Identifiable {
  /// The step's identity: its earliest row's id, never reused.
  public var id: String
  /// What kind of thing it did, which its glyph shows.
  public var kind: ViewerActivityKind
  /// How it went: running, its glyph blue and its words semibold; failed, its words orange.
  public var mark: ViewerStepMark
  /// What was done.
  public var words: ViewerStepWords
  /// When it was done, such as "10:31", shown only when the minute changed; empty otherwise.
  public var time: String
  /// The screenshot it took, shown in place under its line; nil for every step but a screenshot's.
  public var inPlace: ViewerPictureTile?
  /// Whether it kept a picture of the screen right before it, which its picture button opens.
  public var hasPicture: Bool
  /// Whether the picture sheet shows its picture now, so its picture button takes focus back as the sheet closes.
  public var isCurrent: Bool
  /// Why it did not work and what came next; nil for a step that worked or runs.
  public var problem: ViewerActivityProblem?
  /// Whether it is the action whose outcome is unknown while the helper needs the person: it offers Show Screen and
  /// Recover….
  public var offersRecovery: Bool
  /// Whether it typed a password, whose text Atlas never keeps, which a lock badge says.
  public var notKept: Bool

  /// Creates a step.
  ///
  /// - Parameters:
  ///   - id: Its earliest row's id.
  ///   - kind: What kind of thing it did.
  ///   - mark: How it went; quiet by default.
  ///   - words: What was done.
  ///   - time: When, shown only when the minute changed; none by default.
  ///   - inPlace: The screenshot it took; none by default.
  ///   - hasPicture: Whether it kept a picture right before it; it did not by default.
  ///   - isCurrent: Whether the sheet shows its picture now; it does not by default.
  ///   - problem: Why it did not work; none by default.
  ///   - offersRecovery: Whether it offers recovery; it does not by default.
  ///   - notKept: Whether it typed a password Atlas did not keep; it did not by default.
  public init(
    id: String,
    kind: ViewerActivityKind,
    mark: ViewerStepMark = .quiet,
    words: ViewerStepWords,
    time: String = "",
    inPlace: ViewerPictureTile? = nil,
    hasPicture: Bool = false,
    isCurrent: Bool = false,
    problem: ViewerActivityProblem? = nil,
    offersRecovery: Bool = false,
    notKept: Bool = false
  ) {
    self.id = id
    self.kind = kind
    self.mark = mark
    self.words = words
    self.time = time
    self.inPlace = inPlace
    self.hasPicture = hasPicture
    self.isCurrent = isCurrent
    self.problem = problem
    self.offersRecovery = offersRecovery
    self.notKept = notKept
  }

  /// What VoiceOver reads for it: "Clicked Save, 10:31", with why it did not work and what came next; a password's
  /// typing says it was not kept.
  public var label: String {
    let parts = [words.text, notKept ? "Not kept" : "", time, problem?.why ?? "", problem?.next ?? ""]
    return parts.filter { !$0.isEmpty }
      .joined(separator: ", ")
  }
}
