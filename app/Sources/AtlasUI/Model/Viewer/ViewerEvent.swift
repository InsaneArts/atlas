import CoreGraphics

/// What the person did in the main window.
package enum ViewerEvent: Equatable, Sendable {
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
  /// ⋯: offer the picture's and the helper's commands, in a menu under the button.
  ///
  /// - Parameter below: The button's frame in the window's content, top left first, which the menu opens under.
  case showMore(below: CGRect)
  /// Details: show the column beside the screen, or hide it.
  case toggleDetails
  /// Check Again in Details: read the helper's status now.
  case checkAgain
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
  /// Clear: stop outlining the window.
  case clearOutline
  /// The Screenshots pane's search by time changed.
  ///
  /// - Parameter query: The search as typed.
  case searchShots(String)
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
  /// Copy: put the large screenshot, as taken, on the clipboard.
  case copyShot
  /// Show in Finder: show the large screenshot's file.
  case showShotInFinder
  /// A thumbnail came into view, to be read if it is not yet.
  ///
  /// - Parameter name: The screenshot's file name.
  case thumbnailNeeded(String)
}
