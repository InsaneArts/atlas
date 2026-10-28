import CoreGraphics

/// What the person did in the main window.
public enum ViewerEvent: Equatable, Sendable {
  /// A tab picked, or the mini screen pressed for the Screen tab.
  case selectTab(ViewerTab)
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
  /// Keys: offer the keys this Mac keeps for itself, in a menu under the button.
  ///
  /// - Parameter below: The button's frame in the window's content, top left first, which the menu opens under.
  case showKeys(below: CGRect)
  /// ⋯: offer the commands that act on the window's picture, in a menu under the button.
  ///
  /// - Parameter below: The button's frame in the window's content, top left first, which the menu opens under.
  case showMore(below: CGRect)
  /// The sessions button: offer the host sessions to choose from, in a menu under the button.
  ///
  /// - Parameter below: The button's frame in the window's content, top left first, which the menu opens under.
  case showSessions(below: CGRect)
  /// The sessions button took its place in the window, moved, or went: where the window opens the sessions menu when
  /// it is asked to open it by itself, as from the panel.
  ///
  /// - Parameter frame: The button's frame in the window's content, top left first; nil once the button is gone.
  case placeSessions(CGRect?)
  /// A host session chosen, or every session, whose work Activity and Screenshots show.
  ///
  /// - Parameter choice: Whose work.
  case chooseSession(ViewerSessionChoice)
  /// Details: show the column beside the screen, or hide it.
  case toggleDetails
  /// Check Again in Details: read the helper's status now.
  case checkAgain
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
  /// Clean Up This Session's Work… in Details: read what cleaning up the session's work takes, then ask before it
  /// runs.
  ///
  /// - Parameter session: The session's key.
  case askCleanup(session: String)
  /// The alert's button that goes ahead.
  case alertConfirm
  /// The alert's way out.
  case alertCancel
  /// The Activity pane's search changed.
  ///
  /// - Parameter query: The search as typed.
  case searchActivity(String)
  /// One of the Activity pane's filters picked.
  ///
  /// - Parameter filter: The filter.
  case filterActivity(ViewerActivityFilter)
  /// The Apps pane's search changed.
  ///
  /// - Parameter query: The search as typed.
  case searchApps(String)
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
