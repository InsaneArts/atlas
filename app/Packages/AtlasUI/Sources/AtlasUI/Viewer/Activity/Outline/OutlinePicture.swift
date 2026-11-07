import SwiftUI

/// One picture of the outline in its frame: 16:10 on the recessed ground with a hairline rim drawn over it, a window
/// capture fitted inside with a soft shadow, a whole-screen capture filling the frame. The thumbnail it draws is read
/// at the frame's size, so the picture stays sharp; nothing shows until it is read.
struct OutlinePicture: View {
  /// The picture.
  let tile: ViewerPictureTile
  /// The frame's size.
  let size: CGSize
  /// How far a window capture stands inside the frame.
  let inset: CGFloat
  /// Whether the pointer is over it, which strengthens its rim.
  var isHovered = false
  @Environment(\.motionReduced) private var reduceMotion

  /// 4: how far the current picture's ring and a focus ring stand outside the frame: a gap of 2, then 2 of ring.
  static let ringReach = Spacing.half + Stroke.thick

  /// The frame, its picture and its rim.
  var body: some View {
    let shape = RoundedRectangle(cornerRadius: ViewerLayout.pictureRadius, style: .continuous)
    Color.groundRecessed
      .overlay {
        if let thumbnail = tile.thumbnail {
          picture(thumbnail)
            .transition(.opacity)
        }
      }
      .animation(Motion.animation(Motion.fade, reduceMotion: reduceMotion), value: tile.thumbnail)
      .frame(width: size.width, height: size.height)
      .clipShape(shape)
      .overlay {
        shape.strokeBorder(
          isHovered ? Color.strokeHover : Color.strokeContent,
          lineWidth: isHovered ? Stroke.regular : Stroke.hairline
        )
      }
      .overlay {
        if tile.isCurrent {
          // The card's own ground shows in the gap between the frame and the ring.
          RoundedRectangle(cornerRadius: ViewerLayout.pictureRadius + Self.ringReach, style: .continuous)
            .strokeBorder(Color.accent, lineWidth: Stroke.thick)
            .padding(-Self.ringReach)
            .allowsHitTesting(false)
        }
      }
  }

  /// The picture: filling the frame for the whole screen, else fitted inside it with a soft shadow.
  ///
  /// - Parameter thumbnail: The picture as read.
  /// - Returns: The picture.
  @ViewBuilder private func picture(_ thumbnail: DesktopPicture) -> some View {
    let image = Image(decorative: thumbnail.image, scale: 1)
      .resizable()
      .interpolation(.high)
    if tile.fillsFrame {
      image.aspectRatio(contentMode: .fill)
    } else {
      image.aspectRatio(contentMode: .fit)
        .elevation(.selection)
        .padding(inset)
    }
  }
}
