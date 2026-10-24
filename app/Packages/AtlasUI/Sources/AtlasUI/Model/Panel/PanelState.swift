/// Everything the menu bar panel draws, decided before it reaches the view (MacPanel, v3 P1–P11).
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
  /// The row of buttons at the foot: Stop while an agent works, and the main action.
  public var controls: PanelControls

  /// Creates the panel's state.
  ///
  /// - Parameters:
  ///   - status: The helper's status, with its line; an empty line hides its row.
  ///   - notice: What the last action said when it did not work; nil shows the status's line.
  ///   - isMoreOpen: Whether the ⋯ menu is open.
  ///   - picture: The helper's screen and how it looks, or why there is none.
  ///   - recent: The newest actions, newest first, at most three; empty hides Recent.
  ///   - controls: The row of buttons at the foot.
  public init(
    status: StatusSummary,
    notice: String? = nil,
    isMoreOpen: Bool = false,
    picture: PanelPictureState,
    recent: [PanelRecentEntry],
    controls: PanelControls
  ) {
    self.status = status
    self.notice = notice
    self.isMoreOpen = isMoreOpen
    self.picture = picture
    self.recent = recent
    self.controls = controls
  }
}
