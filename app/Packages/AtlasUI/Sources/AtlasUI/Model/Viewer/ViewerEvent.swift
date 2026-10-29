/// What the person did in the main window.
public enum ViewerEvent: Equatable, Sendable {
  /// A place picked, from the View menu, or the mini screen pressed for the Screen.
  case show(ViewerPlace)
  /// A Library row pressed: its list, or the Screen again when its list already shows.
  ///
  /// - Parameter place: The row's list.
  case chooseLibrary(ViewerPlace)
  /// A command chosen from one of the window's context menus.
  ///
  /// - Parameter command: The command.
  case menu(ViewerCommand)
  /// The pause button.
  case togglePause
  /// Take Control.
  case takeControl
  /// Give Back.
  case giveBack
  /// Stop: end an agent's input under way in the helper and refuse its next actions.
  case stopAgent
  /// Resume: let agents act in the helper again.
  case resumeAgent
  /// A host session chosen in the sessions column, or All sessions, whose work Activity and Screenshots show.
  ///
  /// - Parameter choice: Whose work.
  case chooseSession(ViewerSessionChoice)
  /// The sidebar toggle: hide the sidebar, or show it again, over the card while the window is too narrow for it.
  case toggleSidebar
  /// The sidebar shown over the card put away: by a click on the card or Escape.
  case dismissSidebarOverlay
  /// Ended's heading: list every session that ended, or fold them away again.
  case toggleEnded
  /// The sidebar's search circle: open the search field and give it the focus.
  case openSearch
  /// The sidebar's search, as the person typed it, which narrows the sessions and the Activity or Apps list shown.
  ///
  /// - Parameter query: The query; empty keeps everything.
  case search(String)
  /// The search given up: its query emptied and the field closed back to its circle.
  case closeSearch
  /// Open Settings on Helper to see what the helper needs.
  case openSettings
  /// Recover…: ask before recovering the helper.
  case askRecover
  /// Switch to the Helper…: ask before switching this Mac's screen to the helper.
  case askSwitch
  /// Log Out the Helper…: ask before logging the helper out.
  case askLogOut
  /// Restart the Helper…: ask before restarting it.
  case askRestart
  /// The alert's button that goes ahead.
  case alertConfirm
  /// The alert's way out.
  case alertCancel
  /// One of the Activity pane's filters picked.
  ///
  /// - Parameter filter: The filter.
  case filterActivity(ViewerActivityFilter)
  /// An Activity moment opened or folded by the person, which keeps their choice for the window's life.
  ///
  /// - Parameters:
  ///   - id: The moment's identity.
  ///   - opens: True to open it, false to fold it.
  case toggleMoment(String, opens: Bool)
  /// "Show N more" pressed under an open Activity moment, which then lists every step it holds.
  ///
  /// - Parameter id: The moment's identity.
  case showMomentSteps(String)
  /// Show on screen: outline a window on the Screen tab.
  ///
  /// - Parameters:
  ///   - app: The app's process id.
  ///   - window: The window's number.
  case showWindow(app: Int, window: Int)
  /// The outlined window's ×, or Escape: stop outlining it.
  case clearOutline
  /// The small screen let go of in a corner, which the window remembers.
  ///
  /// - Parameter corner: The corner it settled in.
  case moveCorner(MiniCorner)
  /// How many screenshots stand in each of the grid's rows now, which ↑ and ↓ step by.
  ///
  /// - Parameter columns: The grid's columns.
  case shotColumns(Int)
  /// A screenshot picked in the grid.
  ///
  /// - Parameter name: The screenshot's file name.
  case selectShot(String)
  /// Show a screenshot large, from the grid or an Activity line's Open.
  ///
  /// - Parameter name: The screenshot's file name.
  case openShot(String)
  /// Space: show the selected screenshot large, or close the large view.
  case toggleShot
  /// Previous, or ←: the screenshot before.
  case previousShot
  /// Next, or →: the screenshot after.
  case nextShot
  /// Close, or Esc: close the large view.
  case closeShot
  /// ↑: the screenshot a row above the selected one.
  case shotAbove
  /// ↓: the screenshot a row below the selected one.
  case shotBelow
  /// Esc with no screenshot shown large: no screenshot selected.
  case clearShotSelection
  /// Copy: put the large screenshot, as taken, on the clipboard.
  case copyShot
  /// Show in Finder: show the large screenshot's file.
  case showShotInFinder
  /// A thumbnail came into view, to be read if it is not yet.
  ///
  /// - Parameter name: The screenshot's file name.
  case thumbnailNeeded(String)
}
