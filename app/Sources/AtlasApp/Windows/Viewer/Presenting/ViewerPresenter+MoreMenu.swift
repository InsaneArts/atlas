import AtlasUI

extension ViewerPresenter {
  /// The ⋯ menu: only what acts on this window's picture, so nothing in it waits for long. Copy and Save act on the
  /// screenshot shown large or selected while the Screenshots tab shows one.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The menu.
  static func moreMenu(_ snapshot: ViewerSnapshot) -> ViewerMoreMenu {
    ViewerMoreMenu(
      hasPicture: snapshot.reading.picture != nil,
      actsOnShot: actsOnShot(snapshot),
      actualSize: snapshot.session.actualSize,
      rate: snapshot.rate
    )
  }

  /// Whether Copy and Save act on a screenshot now: the Screenshots tab shows, with one shown large or selected.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: Whether they do.
  static func actsOnShot(_ snapshot: ViewerSnapshot) -> Bool {
    guard snapshot.session.tab == .screenshots else { return false }
    return snapshot.panes.shots.large != nil || selectedShot(snapshot) != nil
  }
}
