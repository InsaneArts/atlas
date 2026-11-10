import AtlasUI

/// The sessions, as the sidebar and the Session menu choose them: every session's work, the one over or under the one
/// chosen in the sidebar's order, a session's Activity or screenshots from its menu, and Clean Up for the one chosen.
/// A session chosen in the sidebar or by Previous or Next Session, or All, narrows Activity, and from the Screen opens
/// it.
extension ViewerController {
  /// A row chosen in the sidebar, All included: whose work Activity narrows to. From the Screen it opens Activity, the
  /// picture shrinking into the corner, the row chosen already included, unless this window holds control, which
  /// leaving the Screen would give back.
  ///
  /// - Parameter choice: Whose work.
  func openSession(_ choice: ViewerSessionChoice) {
    chooseSession(choice)
    if Self.opensActivity(from: session.place, holdingControl: holdingControl) { show(.activity) }
  }

  /// Whether choosing a session, or All, opens Activity: from the Screen while this window doesn't hold control.
  /// Activity, when it shows, stays, narrowed to the choice.
  ///
  /// - Parameters:
  ///   - place: What the window shows.
  ///   - holdingControl: Whether this window holds control.
  /// - Returns: Whether Activity opens.
  static func opensActivity(from place: ViewerPlace, holdingControl: Bool) -> Bool {
    place == .screen && !holdingControl
  }

  /// All Sessions: every session's work.
  func chooseAllSessions() {
    chooseSession(.all)
  }

  /// Previous Session or Next Session: the session `offset` rows from the one chosen, in the order the sidebar draws
  /// them, held at its first and last rows, as if its row were chosen.
  ///
  /// - Parameter offset: How many rows down; negative goes up.
  func chooseSession(by offset: Int) {
    let snapshot = snapshot
    let sessions = ViewerPresenter.sessions(snapshot)
    openSession(sessions.choice(offset, from: snapshot.shownSessions))
  }

  /// A session's work in a place, such as Activity, as its menu's Show Activity offers it.
  ///
  /// - Parameters:
  ///   - place: The place, such as Activity.
  ///   - key: The session's key.
  func show(_ place: ViewerPlace, of key: String) {
    chooseSession(.one(key))
    show(place)
  }

  /// The Session menu's Clean Up This Session's Work…, for the session chosen, as its own menu offers it.
  func askCleanupChosen() {
    guard case .one(let key) = snapshot.shownSessions else { return }
    perform(.askCleanup(session: key))
  }
}
