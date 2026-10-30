/// Everything the main window draws, decided before it reaches the view.
public struct ViewerState: Equatable, Sendable {
  /// The content card's header: who has the helper.
  public var header: ViewerHeaderState
  /// The Screen: the helper's screen and what is laid over it.
  public var screen: ViewerScreenState
  /// The content card's footer: the newest thing to know and the buttons.
  public var footer: ViewerFooterState
  /// The sidebar at the leading edge: the sessions, the Library and the helper's health.
  public var sidebar: ViewerSidebarState
  /// The alert over the whole window; nil shows none.
  public var alert: AlertCardState<AlertChoice>?
  /// The helper's screen, small and live in the corner, over Activity, Apps or Screenshots; nil shows none.
  public var mini: MiniScreenState?
  /// The corner the small screen keeps to, as the person last left it.
  public var miniCorner: MiniCorner
  /// What the card shows between its header and footer: the screen, or a list from the Library.
  public var pane: ViewerPane
  /// What VoiceOver says about the helper's state: only a state that asks something of the person, in the window that
  /// has the keyboard, once it has held for a moment; nil says nothing.
  public var announcement: String?
  /// Whether the window is in full screen, which hides the window buttons the sidebar otherwise keeps clear of.
  public var isFullScreen: Bool
  /// Whether the person is resizing the window now, which holds the pointer's highlights still until they let go.
  public var isResizing: Bool

  /// Creates the main window's state.
  ///
  /// - Parameters:
  ///   - header: The content card's header.
  ///   - screen: The Screen.
  ///   - footer: The content card's footer.
  ///   - sidebar: The sidebar.
  ///   - alert: The alert over the whole window; nil shows none.
  ///   - mini: The small screen over a list; nil shows none.
  ///   - miniCorner: The corner the small screen keeps to; the bottom right by default.
  ///   - pane: What the card shows; the screen by default.
  ///   - announcement: What VoiceOver says about the helper's state; nothing by default.
  ///   - isFullScreen: Whether the window is in full screen; it is not by default.
  ///   - isResizing: Whether the window is being resized; it is not by default.
  public init(
    header: ViewerHeaderState,
    screen: ViewerScreenState,
    footer: ViewerFooterState,
    sidebar: ViewerSidebarState,
    alert: AlertCardState<AlertChoice>? = nil,
    mini: MiniScreenState? = nil,
    miniCorner: MiniCorner = .bottomTrailing,
    pane: ViewerPane = .screen,
    announcement: String? = nil,
    isFullScreen: Bool = false,
    isResizing: Bool = false
  ) {
    self.header = header
    self.screen = screen
    self.footer = footer
    self.sidebar = sidebar
    self.alert = alert
    self.mini = mini
    self.miniCorner = miniCorner
    self.pane = pane
    self.announcement = announcement
    self.isFullScreen = isFullScreen
    self.isResizing = isResizing
  }
}
