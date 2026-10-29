import AtlasUI

/// The sidebar: hiding and showing it, which the next window remembers; showing it over the card while the window is
/// too narrow for it, which the toggle, Find and a choice in it answer; opening and folding Ended; and its search, which
/// Find opens wherever the window is, showing the sidebar first if it is hidden, and which narrows the sessions and the
/// Activity or Apps list shown.
extension ViewerController {
  /// The toggle: puts away the sidebar shown over the card; shows it there while the window is too narrow to show it
  /// beside the card, leaving the person's own choice as it was; else hides the sidebar, or shows it again, and
  /// remembers which for the next window.
  func toggleSidebar() {
    if session.overlayShown { return setOverlayShown(false) }
    if session.fit.sidebar == .hidden { return setOverlayShown(true) }
    setSidebarHidden(!session.sidebarHidden)
  }

  /// Hides or shows the sidebar, saving the choice when it changes; the card's room changes with it, and the content
  /// fits it again.
  ///
  /// - Parameter hidden: Whether the sidebar hides.
  func setSidebarHidden(_ hidden: Bool) {
    guard hidden != session.sidebarHidden else { return }
    session.sidebarHidden = hidden
    dependencies.preferences.saveSidebarHidden(hidden)
    refit()
  }

  /// Shows the sidebar whole over the card, or puts it away; it shows only while the window is too narrow for it
  /// beside the card.
  ///
  /// - Parameter shown: Whether it shows over the card.
  func setOverlayShown(_ shown: Bool) {
    let shown = shown && session.fit.sidebar == .hidden
    if session.overlayShown != shown { session.overlayShown = shown }
  }

  /// Opens Ended, listing every session that ended, or folds it again.
  func toggleEnded() {
    session.endedShown.toggle()
  }

  /// Opens the sidebar's search and gives it the focus, showing the whole sidebar first if it is hidden: beside the
  /// card where the window has room, else over it.
  func openSearch() {
    if session.fit.sidebar == .full { setSidebarHidden(false) } else { setOverlayShown(true) }
    session.search.isOpen = true
    session.search.focusRequests += 1
  }

  /// The search's query, as the person typed it.
  ///
  /// - Parameter query: The query; empty keeps everything.
  func search(_ query: String) {
    if session.search.query != query { session.search.query = query }
  }

  /// Empties the search and closes it back to its circle.
  func closeSearch() {
    session.search.isOpen = false
    session.search.query = ""
  }

  /// Find…: the sidebar's search, on the Screen and on every list.
  func find() {
    openSearch()
  }
}
