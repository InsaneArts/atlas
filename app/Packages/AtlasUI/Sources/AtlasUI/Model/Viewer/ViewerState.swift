/// Everything the main window draws, decided before it reaches the view.
public struct ViewerState: Equatable, Sendable {
  /// The quiet row at the top: the status, the tabs and the buttons.
  public var top: ViewerTopRowState
  /// The Screen tab: the helper's screen and what is laid over it.
  public var screen: ViewerScreenState
  /// The Details column beside the screen; nil hides it.
  public var details: ViewerDetailsState?
  /// The alert over the whole window; nil shows none.
  public var alert: AlertCardState<AlertChoice>?
  /// The helper's screen, small and live in the corner, over the Activity or Apps tab; nil shows none.
  public var mini: MiniScreenState?
  /// What shows under the top row: the screen, or the selected tab's pane.
  public var pane: ViewerPane

  /// Creates the main window's state.
  ///
  /// - Parameters:
  ///   - top: The quiet row at the top.
  ///   - screen: The Screen tab.
  ///   - details: The Details column; nil hides it.
  ///   - alert: The alert over the whole window; nil shows none.
  ///   - mini: The small screen over the Activity or Apps tab; nil shows none.
  ///   - pane: What shows under the top row; the screen by default.
  public init(
    top: ViewerTopRowState,
    screen: ViewerScreenState,
    details: ViewerDetailsState? = nil,
    alert: AlertCardState<AlertChoice>? = nil,
    mini: MiniScreenState? = nil,
    pane: ViewerPane = .screen
  ) {
    self.top = top
    self.screen = screen
    self.details = details
    self.alert = alert
    self.mini = mini
    self.pane = pane
  }
}
