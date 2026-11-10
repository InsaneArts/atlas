/// An app run of the Activity outline: which app and what was done there, its problems, and when. Folded, its problem
/// lines and newest pictures show under it; opened, its steps show in place, oldest first, and it offers Replay.
public struct ViewerOutlineRun: Equatable, Sendable, Identifiable {
  /// Its identity, which holds as newer steps join it.
  public var id: String
  /// The app's icon, or the mark of what it held when it was in no app.
  public var icon: ActivityIcon
  /// The app's name, or "Commands" or "You" for a run in no app.
  public var name: String
  /// What was done there, such as "Typed, pressed keys and clicked · 1,212 actions".
  public var summary: String
  /// Its problems, such as "1 problem"; empty for none.
  public var problems: String
  /// When, such as "01:49–01:50", or "01:51–now" while it runs.
  public var time: String
  /// Whether one of its steps runs now, which a blue dot before its time says.
  public var isRunning: Bool
  /// Whether its steps show.
  public var isOpen: Bool
  /// Whether the person can fold or open it; a run held open, as its action's outcome is unknown, offers neither.
  public var canFold: Bool
  /// Whether its steps kept pictures to replay, which Replay offers while it is open.
  public var canReplay: Bool
  /// Its problem lines, newest first, which show while it is folded.
  public var problemSteps: [ViewerActivityStep]
  /// Its newest pictures, which show while it is folded and its pictures are not opened in place.
  public var strip: ViewerPictureStrip?
  /// Its pictures opened in place, which show in place of the strip.
  public var grid: ViewerPictureGrid?
  /// Its steps, oldest first, while it is open; none while folded.
  public var steps: [ViewerActivityStep]
  /// The one listed account its steps acted in, such as "github.com"; nil for none or more than one.
  public var account: String?
  /// What let its steps act there, when they all say the same, which a badge after its words says; nil otherwise.
  public var allowance: AllowanceBadge?

  /// Creates a run.
  ///
  /// - Parameters:
  ///   - id: Its identity.
  ///   - icon: The app's icon.
  ///   - name: The app's name.
  ///   - summary: What was done there.
  ///   - problems: Its problems; none by default.
  ///   - time: When.
  ///   - isRunning: Whether a step runs now; none does by default.
  ///   - isOpen: Whether its steps show; folded by default.
  ///   - canFold: Whether the person can fold or open it; they can by default.
  ///   - canReplay: Whether its steps kept pictures to replay; they did not by default.
  ///   - problemSteps: Its problem lines; none by default.
  ///   - strip: Its newest pictures; none by default.
  ///   - grid: Its pictures opened in place; none by default.
  ///   - steps: Its steps while open; none by default.
  ///   - account: The one listed account its steps acted in; none by default.
  ///   - allowance: What let its steps act there; none by default.
  public init(
    id: String,
    icon: ActivityIcon,
    name: String,
    summary: String,
    problems: String = "",
    time: String,
    isRunning: Bool = false,
    isOpen: Bool = false,
    canFold: Bool = true,
    canReplay: Bool = false,
    problemSteps: [ViewerActivityStep] = [],
    strip: ViewerPictureStrip? = nil,
    grid: ViewerPictureGrid? = nil,
    steps: [ViewerActivityStep] = [],
    account: String? = nil,
    allowance: AllowanceBadge? = nil
  ) {
    self.id = id
    self.icon = icon
    self.name = name
    self.summary = summary
    self.problems = problems
    self.time = time
    self.isRunning = isRunning
    self.isOpen = isOpen
    self.canFold = canFold
    self.canReplay = canReplay
    self.problemSteps = problemSteps
    self.strip = strip
    self.grid = grid
    self.steps = steps
    self.account = account
    self.allowance = allowance
  }

  /// Whether it stands in one box with its key items: folded, with problem lines or pictures under its row.
  public var isBoxed: Bool {
    !isOpen && grid == nil && (!problemSteps.isEmpty || strip != nil)
  }

  /// What VoiceOver reads for it: "TextEdit, Typed and clicked · 9 actions, 1 problem, 10:21–10:24", with the account
  /// it acted in, unless it is titled by it, and what let it act there when it says so.
  public var label: String {
    [name, account == name ? "" : account ?? "", summary, problems, allowance?.words ?? "", time]
      .filter { !$0.isEmpty }
      .joined(separator: ", ")
  }
}
