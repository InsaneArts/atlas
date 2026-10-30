/// A moment of the Activity list: one stream's run of work in one app, headed by the app's icon and name. Open, its
/// steps follow and its header says when it started; folded, it is one capsule row that says how much it held and
/// when, with its age at the end. A moment held open, as while one of its steps runs, offers no fold.
public struct ViewerActivityMoment: Equatable, Sendable, Identifiable {
  /// The moment's identity, which holds as newer steps join it.
  public var id: String
  /// What leads it: the app's icon, or the mark of what it holds when it was in no app.
  public var icon: ActivityIcon
  /// Its title: the app's name, or "Commands", "Screenshots" or "You" for a moment in no app.
  public var title: String
  /// Whose work it was, beside the title while every session's work shows; nil with one session chosen.
  public var session: ViewerMomentSession?
  /// When it started, on the person's clock, such as "10:31", which an open header shows.
  public var time: String
  /// How much it held and when, such as "9 actions · 10:21–10:24", which a folded row shows.
  public var span: String
  /// How long ago it ended, such as "7 min", at a folded row's end.
  public var age: String
  /// Whether its steps show.
  public var isOpen: Bool
  /// Whether the person can fold or open it; a moment held open offers neither.
  public var canFold: Bool

  /// Its shown steps, newest first; empty while folded.
  public var steps: [ViewerActivityStep]
  /// The control for the steps not yet shown, when there are any.
  public var more: ViewerMoreSteps?

  /// Creates a moment.
  ///
  /// - Parameters:
  ///   - id: Its identity.
  ///   - icon: What leads it.
  ///   - title: Its title.
  ///   - session: Whose work it was; none by default.
  ///   - time: When it started.
  ///   - span: How much it held and when.
  ///   - age: How long ago it ended.
  ///   - isOpen: Whether its steps show.
  ///   - canFold: Whether the person can fold or open it; they can by default.
  ///   - steps: Its shown steps.
  ///   - more: Its optional Show more control.
  public init(
    id: String,
    icon: ActivityIcon,
    title: String,
    session: ViewerMomentSession? = nil,
    time: String,
    span: String,
    age: String,
    isOpen: Bool,
    canFold: Bool = true,
    steps: [ViewerActivityStep] = [],
    more: ViewerMoreSteps? = nil
  ) {
    self.id = id
    self.icon = icon
    self.title = title
    self.session = session
    self.time = time
    self.span = span
    self.age = age
    self.isOpen = isOpen
    self.canFold = canFold
    self.steps = steps
    self.more = more
  }

  /// What VoiceOver reads for it: "Opie, Claude Code · understudy, 10:31", or folded "TextEdit, 9 actions ·
  /// 10:21–10:24, 7 min ago".
  public var label: String {
    let when = isOpen ? [time] : [span, age.isEmpty ? "" : "\(age) ago"]
    return ([title, session?.name ?? ""] + when)
      .filter { !$0.isEmpty }
      .joined(separator: ", ")
  }
}
