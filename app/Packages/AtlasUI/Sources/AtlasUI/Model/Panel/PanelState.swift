/// Everything the menu bar panel draws, decided before it reaches the view.
public struct PanelState: Equatable, Sendable {
  /// The helper's status, which leads the panel: its mark and word as the heading, and the line under them; an empty
  /// line hides its row.
  public var status: StatusSummary
  /// What the last action of the panel's own said when it did not work, shown in place of the status's line until
  /// the next action or the next opening; nil shows the line.
  public var notice: String?
  /// Whether the ⋯ menu is open, which draws its button in the header pressed.
  public var isMoreOpen: Bool
  /// The helper's screen and how it looks, or why there is none.
  public var picture: PanelPictureState
  /// The newest actions, newest first, at most three; empty hides Recent.
  public var recent: [PanelRecentEntry]
  /// The live host sessions, at most ``mostSessions``, while any is live; when there are any, they take Recent's
  /// place, whatever `recent` holds.
  public var sessions: [PanelSessionEntry]
  /// The row under the sessions that opens every one, such as "+2 more", while there are more than the rows show;
  /// nil for none.
  public var moreSessions: String?
  /// The row of buttons at the foot: Stop while an agent works, and the main action.
  public var controls: PanelControls
  /// What VoiceOver should say about the helper now: a state that asks something of the person, once it has held;
  /// nil for nothing.
  public var announcement: String?

  /// The most session rows the panel lists; the rest go under one row that opens them all.
  public static let mostSessions = PanelLayout.mostSessions

  /// Creates the panel's state.
  ///
  /// - Parameters:
  ///   - status: The helper's status, with its line; an empty line hides its row.
  ///   - notice: What the last action said when it did not work; nil shows the status's line.
  ///   - isMoreOpen: Whether the ⋯ menu is open.
  ///   - picture: The helper's screen and how it looks, or why there is none.
  ///   - recent: The newest actions, newest first, at most three; empty hides Recent.
  ///   - sessions: The live host sessions, at most ``mostSessions``; none by default, which shows Recent.
  ///   - moreSessions: The row that opens every session; none by default.
  ///   - controls: The row of buttons at the foot.
  ///   - announcement: What VoiceOver should say about the helper now; nil by default.
  public init(
    status: StatusSummary,
    notice: String? = nil,
    isMoreOpen: Bool = false,
    picture: PanelPictureState,
    recent: [PanelRecentEntry],
    sessions: [PanelSessionEntry] = [],
    moreSessions: String? = nil,
    controls: PanelControls,
    announcement: String? = nil
  ) {
    self.status = status
    self.notice = notice
    self.isMoreOpen = isMoreOpen
    self.picture = picture
    self.recent = recent
    self.sessions = sessions
    self.moreSessions = moreSessions
    self.controls = controls
    self.announcement = announcement
  }
}
