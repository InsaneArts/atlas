import AtlasService

/// Picking the desktop the window shows, and following the desktop shown as it moves, from here or anywhere else.
extension ViewerController {
  /// Shows `desktop` from now on: this window's control is given back first, then the window follows the move. What
  /// keeps it from being shown is said on the footer's line.
  ///
  /// - Parameter desktop: The desktop's ID.
  func view(_ desktop: String) async {
    if let refused = await actions.view(desktop) {
      say(Words.sentence(refused))
      return
    }
    followDesktops()
  }

  /// Follows a move of the desktop shown, once: the picture, its frame and what the window read of the desktop shown
  /// before go, Activity and the sessions are read again for the one shown now, every session's work showing, and the
  /// status is read at once. Nothing happens while the desktop shown has not moved since the window last followed it.
  func followDesktops() {
    let moves = actions.desktops.moves
    guard moves != desktopMoves else { return }
    desktopMoves = moves
    readingLoop.forgetDesktop()
    if session.chosenSession != .all { chooseSession(.all) }
    panesModel.sendPane(.closePicture)
    panesModel.followSessions(readNow: true)
    panesModel.followPane()
    readingLoop.refresh()
  }
}
