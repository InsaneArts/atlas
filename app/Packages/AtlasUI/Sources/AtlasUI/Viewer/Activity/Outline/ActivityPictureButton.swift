import SwiftUI

/// One framed picture of the outline that opens in the picture sheet, reading its thumbnail as it comes into view.
struct ActivityPictureButton: View {
  /// The picture.
  let tile: ViewerPictureTile
  /// The frame's size.
  let size: CGSize
  /// How far a window capture stands inside the frame.
  let inset: CGFloat
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The picture's button.
  var body: some View {
    Button {
      send(.openShot(tile.id))
    } label: {
      Color.clear
        .frame(width: size.width, height: size.height)
    }
    .buttonStyle(OutlinePictureStyle(tile: tile, size: size, inset: inset))
    .help("Open this picture")
    .accessibilityLabel(tile.label)
    .accessibilityHint(Text("Opens the picture"))
    .asksForThumbnail(tile.id, send: send)
  }
}
