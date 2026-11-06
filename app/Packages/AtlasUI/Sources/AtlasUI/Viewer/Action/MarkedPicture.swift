import SwiftUI

/// A picture of the helper's screen with an action's mark drawn over it, filling the frame it is given at its own
/// shape; black until it arrives.
struct MarkedPicture: View {
  /// The picture; nil until it is read.
  let picture: DesktopPicture?
  /// The mark over it; nil draws none.
  let mark: ViewerActionMark?

  /// The picture, then the mark.
  var body: some View {
    Color.letterbox
      .overlay {
        if let picture {
          Image(decorative: picture.image, scale: 1)
            .resizable()
            .interpolation(.high)
        }
      }
      .overlay {
        if let mark, picture != nil {
          ActionMarkLayer(mark: mark)
        }
      }
  }
}
