/// What the person did in the main window.
public enum ViewerEvent: Equatable, Sendable {
  /// A place picked, from the View menu, or the mini screen pressed for the Screen.
  case show(ViewerPlace)
  /// A command chosen from one of the window's context menus.
  ///
  /// - Parameter command: The command.
  case menu(ViewerCommand)
  /// The pause button.
  case togglePause
  /// The sound button: mute the helper, or let it play.
  case toggleSound
  /// Take Control.
  case takeControl
  /// Give Back.
  case giveBack
  /// Stop: end an agent's input under way in the helper and refuse its next actions.
  case stopAgent
  /// Resume: let agents act in the helper again.
  case resumeAgent
  /// A host session chosen in the sessions column, or All sessions: Activity shows its work.
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
  /// The sidebar's search, as the person typed it, which narrows the sessions and Activity.
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
  /// An Activity session opened or folded by the person, which keeps their choice for the window's life.
  ///
  /// - Parameters:
  ///   - id: The session's identity.
  ///   - opens: True to open it, false to fold it.
  case toggleSession(String, opens: Bool)
  /// An Activity run opened or folded by the person, which keeps their choice for the window's life.
  ///
  /// - Parameters:
  ///   - id: The run's identity.
  ///   - opens: True to open it, false to fold it.
  case toggleRun(String, opens: Bool)
  /// A run's "120 pictures" pressed: its pictures open in place.
  ///
  /// - Parameter id: The run's identity.
  case showRunPictures(String)
  /// "Show N more" under a run's pictures: every one of them shows.
  ///
  /// - Parameter id: The run's identity.
  case showAllRunPictures(String)
  /// Show fewer under a run's pictures: they fold back to the run's newest few.
  ///
  /// - Parameter id: The run's identity.
  case hideRunPictures(String)
  /// "46 more sessions on Wednesday" pressed: every session of that day shows.
  ///
  /// - Parameter id: The day's identity.
  case showDaySessions(String)
  /// The small screen let go of in a corner, which the window remembers.
  ///
  /// - Parameter corner: The corner it settled in.
  case moveCorner(MiniCorner)
  /// A screenshot pressed in the outline: it opens in the picture sheet.
  ///
  /// - Parameter name: The screenshot's file name.
  case openShot(String)
  /// A step's picture button pressed: the screen just before it opens in the picture sheet.
  ///
  /// - Parameter id: The step's identity.
  case openAction(String)
  /// ‹, or ←: the run's picture before.
  case previousPicture
  /// ›, or →: the run's picture after.
  case nextPicture
  /// Before or After picked: which of a step's pictures the sheet shows.
  ///
  /// - Parameter side: The side.
  case pictureSide(ViewerPictureSide)
  /// Copy: put the sheet's picture, as taken, on the clipboard.
  case copyPicture
  /// Show in Finder: show the sheet's picture's file.
  case showPictureInFinder
  /// Done, Return or Escape: close the sheet, back where the picture was opened.
  case closePicture
  /// An opened run's Replay: its steps played over the whole window.
  ///
  /// - Parameter id: The run's identity.
  case openReplay(String)
  /// A frame of the replay picked, from its strip or its bar.
  ///
  /// - Parameter index: The frame.
  case replayFrame(Int)
  /// Previous, or ←: the replay's frame before.
  case previousFrame
  /// Next, or →: the replay's frame after.
  case nextFrame
  /// Play or Pause, or Space.
  case togglePlay
  /// Done, or Esc: close the replay.
  case closeReplay
  /// A thumbnail came into view, to be read if it is not yet.
  ///
  /// - Parameter name: The screenshot's file name.
  case thumbnailNeeded(String)
  /// An answer to the question over the Screen.
  ///
  /// - Parameters:
  ///   - id: The question's id.
  ///   - choice: The answer.
  case answerAsk(id: String, choice: AskChoice)
  /// Done on the signing-in bar: the person finished in the helper's browser.
  case signInDone
  /// Cancel on the signing-in bar: the hold ends and nothing is added.
  case signInCancel
}
