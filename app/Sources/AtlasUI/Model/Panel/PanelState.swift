/// Everything the menu bar panel draws, decided before it reaches the view (MacPanel).
package struct PanelState: Equatable, Sendable {
  /// The helper's status: the word and mark on the right of the header, and the line under it; an empty line hides
  /// its row.
  package var status: StatusSummary
  /// What the last action of the panel's own said when it did not work, shown in place of the status's line until
  /// the next action or the next opening; nil shows the line.
  package var notice: String?
  /// The helper's screen, or why there is none.
  package var picture: PanelPictureState
  /// The newest actions, newest first, at most three; empty hides Recent.
  package var recent: [ActivityEntry]
  /// The row of buttons at the foot.
  package var controls: PanelControls

  /// Creates the panel's state.
  ///
  /// - Parameters:
  ///   - status: The helper's status, with its line; an empty line hides its row.
  ///   - notice: What the last action said when it did not work; nil shows the status's line.
  ///   - picture: The helper's screen, or why there is none.
  ///   - recent: The newest actions, newest first, at most three; empty hides Recent.
  ///   - controls: The row of buttons at the foot.
  package init(
    status: StatusSummary,
    notice: String? = nil,
    picture: PanelPictureState,
    recent: [ActivityEntry],
    controls: PanelControls
  ) {
    self.status = status
    self.notice = notice
    self.picture = picture
    self.recent = recent
    self.controls = controls
  }
}
