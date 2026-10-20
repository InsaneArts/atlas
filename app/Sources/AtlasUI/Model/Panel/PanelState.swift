/// Everything the menu bar panel draws, decided before it reaches the view.
package struct PanelState: Equatable, Sendable {
  /// The helper's mode, for the badge beside the title.
  package var status: StatusSummary
  /// Why the helper is in its mode, under the title; nil hides the line.
  package var reason: String?
  /// What fills the space under the header: the helper's desktop, a placeholder, or the way to log in.
  package var stage: PanelStage
  /// The recovery card's detail; nil hides the card.
  package var recovery: String?
  /// The helper, its display and the last action.
  package var facts: [Fact]
  /// Whether Open Viewer is the next step; beside the way to log in, it steps back.
  package var isViewerPrimary: Bool
  /// The button beside Open Viewer; nil shows none.
  package var secondary: PanelSecondaryAction?
  /// Where logging out stands: nothing, the confirmation, or the offer to force it.
  package var logOut: PanelLogOut
  /// What the last action said; nil shows nothing.
  package var feedback: Feedback?
  /// The menu rows under the cards.
  package var menu: PanelMenu

  /// Creates the panel's state.
  ///
  /// - Parameters:
  ///   - status: The helper's mode, for the badge beside the title.
  ///   - reason: Why the helper is in its mode, under the title; nil hides the line.
  ///   - stage: What fills the space under the header: the helper's desktop, a placeholder, or the way to log in.
  ///   - recovery: The recovery card's detail; nil hides the card.
  ///   - facts: The helper, its display and the last action.
  ///   - isViewerPrimary: Whether Open Viewer is the next step; beside the way to log in, it steps back.
  ///   - secondary: The button beside Open Viewer; nil shows none.
  ///   - logOut: Where logging out stands: nothing, the confirmation, or the offer to force it.
  ///   - feedback: What the last action said; nil shows nothing.
  ///   - menu: The menu rows under the cards.
  package init(
    status: StatusSummary,
    reason: String? = nil,
    stage: PanelStage,
    recovery: String? = nil,
    facts: [Fact],
    isViewerPrimary: Bool,
    secondary: PanelSecondaryAction? = nil,
    logOut: PanelLogOut,
    feedback: Feedback? = nil,
    menu: PanelMenu
  ) {
    self.status = status
    self.reason = reason
    self.stage = stage
    self.recovery = recovery
    self.facts = facts
    self.isViewerPrimary = isViewerPrimary
    self.secondary = secondary
    self.logOut = logOut
    self.feedback = feedback
    self.menu = menu
  }
}
