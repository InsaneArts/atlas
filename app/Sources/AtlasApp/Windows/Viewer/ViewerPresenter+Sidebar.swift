import AtlasKit
import AtlasUI

extension ViewerPresenter {
  /// The sidebar: the helper and why it is in its mode, the recent actions, the readiness checks, the session's
  /// facts, the refresh rate and the picture's note.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The sidebar.
  static func sidebar(_ snapshot: ViewerSnapshot) -> ViewerSidebarState {
    let reading = snapshot.reading.statusReading
    let holdingControl = snapshot.session.holdingControl
    let facts = reading.facts(holdingControl: holdingControl)
    return ViewerSidebarState(
      account: account(snapshot.reading.status),
      reason: reading.reason(holdingControl: holdingControl),
      recent: reading.recentEntries,
      ready: reading.readinessItems,
      facts: [
        Fact(name: Words.displayFact, value: facts.display),
        Fact(name: ViewerWords.inputFact, value: facts.input),
        Fact(name: ViewerWords.controlFact, value: facts.control),
        Fact(name: ViewerWords.pendingFact, value: facts.pending),
      ],
      rate: snapshot.rate,
      pictureNote: pictureNote(snapshot.reading.picture)
    )
  }

  /// The picture's size in pixels, which a Retina picture's points are not, before the note that pictures are kept
  /// in memory only.
  ///
  /// - Parameter picture: The picture shown, if any.
  /// - Returns: The note.
  static func pictureNote(_ picture: DesktopPicture?) -> String {
    guard let picture, picture.image.width > 0 else { return ViewerWords.pictureMemory }
    return ViewerWords.pictureNote(width: picture.image.width, height: picture.image.height)
  }
}
