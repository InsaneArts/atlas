/// One frame of a task's replay: a step's marked picture before it, or the screen after the last step, with what the
/// step did beneath it.
public struct ViewerReplayFrame: Equatable, Sendable, Identifiable {
  /// The frame's identity: its step's, or "after" for the last frame.
  public var id: String
  /// The picture: as taken once read, its thumbnail before then.
  public var picture: ViewerActivityShot
  /// The mark over it; nil for the screen after, or when nothing was recorded to place one.
  public var mark: ViewerActionMark?
  /// What the step did; for the screen after, that it is the screen after the last step.
  public var words: ViewerStepWords
  /// The line under the words, such as "Chrome · 10:30:41 to 10:30:52 · 12 scrolls in one step".
  public var detail: String
  /// The step's kind, which its disc shows; nil for the screen after.
  public var kind: ViewerActivityKind?
  /// How the step went, which its disc's colour shows.
  public var stepMark: ViewerStepMark
  /// How many actions the step holds, which a count on its thumbnail shows past one.
  public var repeats: Int
  /// What the strip says under its thumbnail: its number, or "After".
  public var label: String
  /// The app it was in, such as "Slack"; for the screen after, the last step's; empty when unknown.
  public var app: String
  /// Whether it is the screen after the last step rather than a step.
  public var isAfter: Bool

  /// Creates a frame.
  ///
  /// - Parameters:
  ///   - id: The frame's identity.
  ///   - picture: The picture.
  ///   - mark: The mark over it; none by default.
  ///   - words: What the step did.
  ///   - detail: The line under the words.
  ///   - kind: The step's kind; none by default.
  ///   - stepMark: How the step went; quiet by default.
  ///   - repeats: How many actions the step holds; one by default.
  ///   - label: What the strip says under it.
  ///   - app: The app it was in; unknown by default.
  ///   - isAfter: Whether it is the screen after the last step; it is a step by default.
  public init(
    id: String,
    picture: ViewerActivityShot,
    mark: ViewerActionMark? = nil,
    words: ViewerStepWords,
    detail: String,
    kind: ViewerActivityKind? = nil,
    stepMark: ViewerStepMark = .quiet,
    repeats: Int = 1,
    label: String,
    app: String = "",
    isAfter: Bool = false
  ) {
    self.id = id
    self.picture = picture
    self.mark = mark
    self.words = words
    self.detail = detail
    self.kind = kind
    self.stepMark = stepMark
    self.repeats = repeats
    self.label = label
    self.app = app
    self.isAfter = isAfter
  }
}
