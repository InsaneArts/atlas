import AtlasService

/// Picking the desktop the panel shows, and following the desktop shown as it moves, from here or anywhere else.
extension PanelController {
  /// Shows `desktop` from now on, then reads it; what keeps it from being shown is said in place of the line.
  ///
  /// - Parameter desktop: The desktop's ID.
  func view(_ desktop: String) async {
    session.notice = nil
    if let refused = await actions.view(desktop) { session.notice = Words.sentence(refused) }
    await readStatus()
  }

  /// Reads the desktops, and forgets the picture of the desktop shown before once the one shown has moved, so a
  /// picture of another desktop never shows under this one's status.
  func readDesktops() async {
    await actions.desktops.read()
    let moves = actions.desktops.moves
    guard moves != desktopMoves else { return }
    desktopMoves = moves
    frameReading += 1
    picture = nil
  }

  /// Reads the status at once once the desktop shown has moved from another window, so the menu bar follows it
  /// without waiting for the next reading; nothing while the panel has followed every move.
  func followDesktops() {
    guard actions.desktops.moves != desktopMoves else { return }
    // Unstructured, as a move is told synchronously: the reading runs beside it.
    Task { await readStatus() }
  }

  /// Marks the desktops' menu open or closed, so the popup draws pressed while it is open.
  ///
  /// - Parameter open: Whether the menu is open.
  func setDesktopsOpen(_ open: Bool) {
    session.desktopsOpen = open
  }
}
