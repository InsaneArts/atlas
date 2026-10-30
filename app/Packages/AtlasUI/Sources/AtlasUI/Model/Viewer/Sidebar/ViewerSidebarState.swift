/// The sidebar at the window's leading edge: how it stands, whether it shows whole over the card, the search in its top
/// bar, the host sessions whose work Activity and Screenshots show, the Library at its foot and the helper's health
/// under it.
public struct ViewerSidebarState: Equatable, Sendable {
  /// Whether it shows beside the card or hides.
  public var mode: ViewerSidebarMode
  /// Whether it shows whole over the card, as the toggle or Find shows it while the window is too narrow for it.
  public var overlay: Bool
  /// Whether the window is short, so the Library folds into one row of circles with the helper's dot at its end.
  public var isShort: Bool
  /// The search in its top bar.
  public var search: ViewerSearchState
  /// The sessions.
  public var sessions: ViewerSessionsState
  /// The Library's rows, in the order they stand.
  public var library: [ViewerLibraryRow]
  /// The helper's health, last.
  public var helper: ViewerHelperLine

  /// Creates the sidebar's state.
  ///
  /// - Parameters:
  ///   - mode: Whether it shows beside the card or hides; beside it by default.
  ///   - overlay: Whether it shows whole over the card; no by default.
  ///   - isShort: Whether the Library folds into a row; no by default.
  ///   - search: The search; closed by default.
  ///   - sessions: The sessions.
  ///   - library: The Library's rows.
  ///   - helper: The helper's health.
  public init(
    mode: ViewerSidebarMode = .full,
    overlay: Bool = false,
    isShort: Bool = false,
    search: ViewerSearchState = ViewerSearchState(),
    sessions: ViewerSessionsState,
    library: [ViewerLibraryRow],
    helper: ViewerHelperLine
  ) {
    self.mode = mode
    self.overlay = overlay
    self.isShort = isShort
    self.search = search
    self.sessions = sessions
    self.library = library
    self.helper = helper
  }
}
