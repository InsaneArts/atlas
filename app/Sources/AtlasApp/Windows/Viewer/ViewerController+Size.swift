import AtlasUI
import CoreGraphics

/// The window's size: how the content gives way to it, decided here from each resize and changed only as a threshold is
/// crossed (``ViewerFit``), so a resize at the display's rate changes the window's state only at those steps; and a
/// resize under way, which holds the Screenshots grid's columns, the thumbnails' reads and the pointer's highlights
/// until it ends.
extension ViewerController {
  /// The window was laid out at `size`: the content fits it, and the sidebar shown over the card goes once there is
  /// room for the whole sidebar beside it.
  ///
  /// - Parameter size: The window's size.
  func fitWindow(to size: CGSize) {
    if session.windowSize != size { session.windowSize = size }
    refit()
  }

  /// Fits the content to the window's last size again, as the sidebar is hidden or shown and the card's room changes.
  func refit() {
    guard let size = session.windowSize else { return }
    let fit = session.fit.refitted(to: size, isHidden: session.sidebarHidden)
    if session.fit != fit { session.fit = fit }
    setOverlayShown(session.overlayShown)
  }

  /// A resize began or ended. While it lasts the Screenshots grid keeps the columns it had and the thumbnails wait;
  /// once it ends the grid takes the columns the width holds and the thumbnails waiting are read.
  ///
  /// - Parameter resizing: Whether the resize is under way.
  func setLiveResizing(_ resizing: Bool) {
    guard resizing != session.liveResizing else { return }
    session.liveResizing = resizing
    session.heldShotColumns = resizing ? panes.shots.columns : nil
    panesModel.holdThumbnails(resizing)
  }
}
