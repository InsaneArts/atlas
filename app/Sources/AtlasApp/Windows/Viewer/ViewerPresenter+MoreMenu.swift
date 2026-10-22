import AtlasKit
import AtlasUI

extension ViewerPresenter {
  /// The ⋯ menu (mac.py:347): the picture choice ticked, whether there is a picture to copy or save, the helper's
  /// commands and the note at its foot.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The menu.
  static func moreMenu(_ snapshot: ViewerSnapshot) -> ViewerMoreMenu {
    ViewerMoreMenu(
      picture: pictureChoice(snapshot),
      hasPicture: snapshot.reading.picture != nil,
      helper: snapshot.helperMenu,
      note: ViewerWords.helperNote(
        account: account(snapshot.reading.status),
        display: Words.size(snapshot.reading.status.overview.display)
      )
    )
  }
}
