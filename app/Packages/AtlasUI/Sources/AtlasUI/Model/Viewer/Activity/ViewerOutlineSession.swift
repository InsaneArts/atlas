/// A session of the Activity outline: who worked, on what project, a one-line summary of what they did, and when;
/// then its app runs while open, or its newest pictures while folded. With one session chosen, the card's header
/// names it already, so its row does not show and its runs move up a level.
public struct ViewerOutlineSession: Equatable, Sendable, Identifiable {
  /// Its identity: its stream's and its day's.
  public var id: String
  /// The host's icon; nil for the person's own work, which their mark leads.
  public var icon: RowIcon?
  /// The agent, such as "Claude Code", or "You".
  public var agent: String
  /// The project, such as "atlas"; empty for none.
  public var project: String
  /// What it did, such as "TextEdit and Chrome · 5,044 actions · 341 screenshots".
  public var summary: String
  /// Its problems, such as "1 problem"; empty for none.
  public var problems: String
  /// When, such as "01:08–now".
  public var time: String
  /// Whether it works now, which a blue dot on its icon says.
  public var isWorking: Bool
  /// Whether its runs show.
  public var isOpen: Bool
  /// Whether its row shows; it does not with one session chosen.
  public var showsHead: Bool
  /// Its runs, newest first, while open.
  public var runs: [ViewerOutlineRun]
  /// Its newest pictures while folded.
  public var strip: ViewerPictureStrip?

  /// Creates a session.
  ///
  /// - Parameters:
  ///   - id: Its identity.
  ///   - icon: The host's icon; none for the person's own work.
  ///   - agent: The agent.
  ///   - project: The project; none by default.
  ///   - summary: What it did.
  ///   - problems: Its problems; none by default.
  ///   - time: When.
  ///   - isWorking: Whether it works now; it does not by default.
  ///   - isOpen: Whether its runs show; they do by default.
  ///   - showsHead: Whether its row shows; it does by default.
  ///   - runs: Its runs while open.
  ///   - strip: Its newest pictures while folded; none by default.
  public init(
    id: String,
    icon: RowIcon?,
    agent: String,
    project: String = "",
    summary: String,
    problems: String = "",
    time: String,
    isWorking: Bool = false,
    isOpen: Bool = true,
    showsHead: Bool = true,
    runs: [ViewerOutlineRun],
    strip: ViewerPictureStrip? = nil
  ) {
    self.id = id
    self.icon = icon
    self.agent = agent
    self.project = project
    self.summary = summary
    self.problems = problems
    self.time = time
    self.isWorking = isWorking
    self.isOpen = isOpen
    self.showsHead = showsHead
    self.runs = runs
    self.strip = strip
  }

  /// What VoiceOver reads for it: "Claude Code, atlas, TextEdit · 9 actions, 1 problem, 10:21–now".
  public var label: String {
    [agent, project, summary, problems, time]
      .filter { !$0.isEmpty }
      .joined(separator: ", ")
  }
}
