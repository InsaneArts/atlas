extension ViewerState {
  /// The window as it is drawn at `fit`: the sidebar as the width leaves it, unless the person hid it, and shown over
  /// the card only while the width keeps it from showing whole; the header with or without what its holder does; the
  /// footer's capsules or circles; and the Library whole or folded into a row.
  ///
  /// - Parameter fit: How the content gives way to the window's size.
  /// - Returns: The state, fitted.
  public func fitted(_ fit: ViewerFit) -> ViewerState {
    var state = self
    if state.sidebar.mode != .hidden { state.sidebar.mode = fit.sidebar }
    state.sidebar.overlay = state.sidebar.overlay && fit.sidebar == .hidden
    state.sidebar.isShort = fit.isShort
    state.header.showsWhat = fit.showsWhat
    state.footer.fit = fit.footer
    return state
  }
}
