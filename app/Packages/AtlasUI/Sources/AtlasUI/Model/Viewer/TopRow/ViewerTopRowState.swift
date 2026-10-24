/// The main window's one quiet row at the top: the helper's status on the left, the tabs in the middle and
/// the buttons on the right, on the window's own ground, washed blue while this window holds control.
public struct ViewerTopRowState: Equatable, Sendable {
  /// The helper's state as this window sees it, with its mark and its line.
  public var status: StatusSummary
  /// The tabs, and which one is selected: the only place the selected tab is kept.
  public var tabs: CapsuleTabsState<ViewerTab>
  /// What the Activity tab holds that the person has not seen.
  public var activityUnseen: ViewerUnseen
  /// The buttons on the right.
  public var actions: ViewerTopActions
  /// Whether this window holds control, which washes the row in the accent.
  public var isInControl: Bool
  /// Whether the window is in full screen, which hides the window buttons the status otherwise keeps clear of.
  public var isFullScreen: Bool

  /// Creates the top row's state.
  ///
  /// - Parameters:
  ///   - status: The helper's state as this window sees it.
  ///   - tabs: The tabs, and which one is selected.
  ///   - activityUnseen: What Activity holds unseen; nothing by default.
  ///   - actions: The buttons on the right.
  ///   - isInControl: Whether this window holds control; it does not by default.
  ///   - isFullScreen: Whether the window is in full screen.
  public init(
    status: StatusSummary,
    tabs: CapsuleTabsState<ViewerTab>,
    activityUnseen: ViewerUnseen = .none,
    actions: ViewerTopActions,
    isInControl: Bool = false,
    isFullScreen: Bool
  ) {
    self.status = status
    self.tabs = tabs
    self.activityUnseen = activityUnseen
    self.actions = actions
    self.isInControl = isInControl
    self.isFullScreen = isFullScreen
  }
}
