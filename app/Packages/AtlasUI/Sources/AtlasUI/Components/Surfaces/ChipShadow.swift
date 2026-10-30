import SwiftUI

/// The chip's shadow, drawn only outside its outline, as CSS draws `box-shadow`: a blurred shape cut away
/// under the chip, so nothing darkens the chip or the words on it.
struct ChipShadow: View {
  /// The chip's outline.
  let shape: RoundedRectangle

  /// The component drawn in its current state.

  /// The shadow outside the chip's outline.
  var body: some View {
    shape
      .fill(Color.dropShadowFaint)
      .blur(radius: PictureChipLayout.shadowRadius)
      .offset(y: PictureChipLayout.shadowDrop)
      .mask {
        Rectangle()
          .padding(-PictureChipLayout.shadowRoom)
          .overlay {
            shape
              .blendMode(.destinationOut)
          }
          .compositingGroup()
      }
      .allowsHitTesting(false)
  }
}
