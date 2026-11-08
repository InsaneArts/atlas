/// The sidebar at the window's leading edge: how it stands, whether it shows whole over the card, the search in its top
/// bar, the host sessions whose work Activity shows, and the helper's health at its foot.
public struct ViewerSidebarState: Equatable, Sendable {
  /// Whether it shows beside the card or hides.
  public var mode: ViewerSidebarMode
  /// Whether it shows whole over the card, as the toggle or Find shows it while the window is too narrow for it.
  public var overlay: Bool
  /// Whether the window is short, so the helper's row folds into its mark alone.
  public var isShort: Bool
  /// The search in its top bar.
  public var search: ViewerSearchState
  /// The sessions.
  public var sessions: ViewerSessionsState
  /// The helper's health, last.
  public var helper: ViewerHelperLine

  /// Creates the sidebar's state.
  ///
  /// - Parameters:
  ///   - mode: Whether it shows beside the card or hides; beside it by default.
  ///   - overlay: Whether it shows whole over the card; no by default.
  ///   - isShort: Whether the helper's row folds into its mark; no by default.
  ///   - search: The search; closed by default.
  ///   - sessions: The sessions.
  ///   - helper: The helper's health.
  public init(
    mode: ViewerSidebarMode = .full,
    overlay: Bool = false,
    isShort: Bool = false,
    search: ViewerSearchState = ViewerSearchState(),
    sessions: ViewerSessionsState,
    helper: ViewerHelperLine
  ) {
    self.mode = mode
    self.overlay = overlay
    self.isShort = isShort
    self.search = search
    self.sessions = sessions
    self.helper = helper
  }
}
