/// The main window's one quiet row at the top: the helper's status on the left, the tabs in the middle and the
/// buttons on the right, on the window's own ground.
package struct ViewerTopRowState: Equatable, Sendable {
  /// The helper's state as this window sees it, with its mark and its line.
  package var status: StatusSummary
  /// The tabs, and which one is selected: the only place the selected tab is kept.
  package var tabs: CapsuleTabsState<ViewerTab>
  /// The buttons on the right.
  package var actions: ViewerTopActions
  /// Whether the window is in full screen, which hides the window buttons the status otherwise keeps clear of.
  package var isFullScreen: Bool

  /// Creates the top row's state.
  ///
  /// - Parameters:
  ///   - status: The helper's state as this window sees it.
  ///   - tabs: The tabs, and which one is selected.
  ///   - actions: The buttons on the right.
  ///   - isFullScreen: Whether the window is in full screen.
  package init(
    status: StatusSummary,
    tabs: CapsuleTabsState<ViewerTab>,
    actions: ViewerTopActions,
    isFullScreen: Bool
  ) {
    self.status = status
    self.tabs = tabs
    self.actions = actions
    self.isFullScreen = isFullScreen
  }
}
