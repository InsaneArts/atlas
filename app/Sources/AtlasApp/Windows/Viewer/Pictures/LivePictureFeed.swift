import AtlasUI
import Observation

/// The newest picture of the helper's screen, which every live picture replaces: the one thing that changes with each
/// frame. Only the live slots observe it (``LayerPictureView``), so a frame swaps their layers' contents and presents
/// nothing else of the window. The reading loop writes it alone.
@Observable
final class LivePictureFeed {
  /// The newest picture; nil before the first. A lost picture stays, as the slots drawing it go.
  private(set) var picture: DesktopPicture?

  /// Shows a new picture.
  ///
  /// - Parameter picture: The picture.
  func show(_ picture: DesktopPicture) {
    self.picture = picture
  }

  /// What a live slot draws: what the window lays it out with, holding the newest picture when there is one.
  ///
  /// - Parameter live: What the window's state gives the slot.
  /// - Returns: The same, with the newest picture.
  func drawn(_ live: ViewerLivePicture) -> ViewerLivePicture {
    guard let picture else { return live }
    var drawn = live
    drawn.picture = picture
    return drawn
  }
}
