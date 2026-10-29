import AtlasUI

extension ViewerPresenter {
  /// What acts on the window's picture: only what nothing waits long for. Copy and Save act on the screenshot shown
  /// large or selected while the Screenshots list shows one.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The picture's commands.
  static func pictureMenu(_ snapshot: ViewerSnapshot) -> ViewerPictureMenu {
    ViewerPictureMenu(
      hasPicture: snapshot.reading.picture != nil,
      actsOnShot: actsOnShot(snapshot),
      actualSize: snapshot.session.actualSize,
      rate: snapshot.rate
    )
  }

  /// Whether Copy and Save act on a screenshot now: the Screenshots list shows, with one shown large or selected.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: Whether they do.
  static func actsOnShot(_ snapshot: ViewerSnapshot) -> Bool {
    guard snapshot.session.place == .screenshots else { return false }
    return snapshot.panes.shots.large != nil || selectedShot(snapshot) != nil
  }

  /// What a right click on the picture offers: Copy Picture and Save Picture…; then Actual Size, ticked while on,
  /// Pause Picture or Resume Picture, and Picture Updates with the rate the picture arrives at, or resumes at, ticked.
  /// Nothing while this window holds control, where the click is the helper's, or while there is no picture.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The menu's sections.
  static func screenMenu(_ snapshot: ViewerSnapshot) -> [ViewerMenuSection] {
    let picture = pictureMenu(snapshot)
    guard picture.hasPicture, !snapshot.holdingControl else { return [] }
    let paused = snapshot.session.paused
    let rates = ViewerPictureRate.allCases
      .enumerated()
      .map { index, rate in
        ViewerMenuItem(ViewerWords.rate(rate), command: .chooseRate(index: index), isOn: rate == picture.rate)
      }
    return [
      ViewerMenuSection([
        ViewerMenuItem(ViewerWords.copyPicture, command: .copyPicture),
        ViewerMenuItem(ViewerWords.savePicture, command: .savePicture),
      ]),
      ViewerMenuSection([
        ViewerMenuItem(ViewerWords.actualSizeItem, command: .toggleActualSize, isOn: picture.actualSize),
        ViewerMenuItem(paused ? Words.resumePictureItem : Words.pausePictureItem, command: .togglePause),
        ViewerMenuItem(ViewerWords.updatesItem, children: rates),
      ]),
    ]
  }
}
