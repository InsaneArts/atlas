import SwiftUI

/// A picture as ``OutlinePictureStyle`` draws it.
struct OutlinePictureBody: View {
  /// The picture.
  let tile: ViewerPictureTile
  /// The frame's size.
  let size: CGSize
  @State private var isHovered = false
  @Environment(\.isFocused) private var isFocused
  @Environment(\.hoverPaused) private var hoverPaused

  /// The picture, its rim lit under the pointer, and its focus ring.
  var body: some View {
    OutlinePicture(tile: tile, size: size, isHovered: isHovered && !hoverPaused)
      .overlay {
        if isFocused {
          RoundedRectangle(cornerRadius: ViewerLayout.pictureRadius + OutlinePicture.ringReach, style: .continuous)
            .strokeBorder(Color.strokeFocus, lineWidth: Stroke.thick)
            .padding(-OutlinePicture.ringReach)
        }
      }
      .contentShape(Rectangle())
      .focusEffectDisabled()
      .onHover { isHovered = $0 }
  }
}
