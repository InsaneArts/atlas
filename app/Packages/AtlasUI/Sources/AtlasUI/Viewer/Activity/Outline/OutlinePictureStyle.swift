import SwiftUI

/// How a picture of the outline answers the pointer and the keyboard: under the pointer its rim strengthens in place of
/// lifting; with keyboard focus a ring stands just outside it.
struct OutlinePictureStyle: ButtonStyle {
  /// The picture.
  let tile: ViewerPictureTile
  /// The frame's size.
  let size: CGSize

  /// The picture in its frame.
  ///
  /// - Parameter configuration: Unused but for its label's place.
  /// - Returns: The picture.
  func makeBody(configuration: Configuration) -> some View {
    OutlinePictureBody(tile: tile, size: size)
  }
}
