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
  /// The corner the small screen keeps to, as the person last left it.
  public var miniCorner: MiniCorner
  /// What shows under the top row: the screen, or the selected tab's pane.
  public var pane: ViewerPane
  /// Why Take Control was just refused, in a toast that leaves by itself; nil shows none.
  public var toast: String?
  /// What VoiceOver says about the helper's state: only a state that asks something of the person, in the window that
  /// has the keyboard, once it has held for a moment; nil says nothing.
  public var announcement: String?

  /// Creates the main window's state.
  ///
  /// - Parameters:
  ///   - top: The quiet row at the top.
  ///   - screen: The Screen tab.
  ///   - details: The Details column; nil hides it.
  ///   - alert: The alert over the whole window; nil shows none.
  ///   - mini: The small screen over the Activity or Apps tab; nil shows none.
  ///   - miniCorner: The corner the small screen keeps to; the bottom right by default.
  ///   - pane: What shows under the top row; the screen by default.
  ///   - toast: Why Take Control was just refused; none by default.
  ///   - announcement: What VoiceOver says about the helper's state; nothing by default.
  public init(
    top: ViewerTopRowState,
    screen: ViewerScreenState,
    details: ViewerDetailsState? = nil,
    alert: AlertCardState<AlertChoice>? = nil,
    mini: MiniScreenState? = nil,
    miniCorner: MiniCorner = .bottomTrailing,
    pane: ViewerPane = .screen,
    toast: String? = nil,
    announcement: String? = nil
  ) {
    self.top = top
    self.screen = screen
    self.details = details
    self.alert = alert
    self.mini = mini
    self.miniCorner = miniCorner
    self.pane = pane
    self.toast = toast
    self.announcement = announcement
  }
}
