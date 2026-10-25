import SwiftUI

/// The helper account's picture in a circle, as macOS shows every account picture: the picture fills a square
/// frame and is clipped round, never stretched. Where no picture is supplied, as in previews, the avatar disc
/// stands in.
struct AccountPicture: View {
  /// The circle's side, in points, from its window's layout, such as Choose's 30 or the Helper pane's 40.
  let side: CGFloat
  @Environment(\.accountPicture) private var picture

  /// The picture in its circle, or the avatar disc.
  var body: some View {
    Group {
      if let picture {
        picture
          .resizable()
          .interpolation(.high)
          .scaledToFill()
      } else {
        Color.fillAvatar
      }
    }
    .frame(width: side, height: side)
    .clipShape(Circle())
    .accessibilityHidden(true)
  }
}

#if DEBUG
  #Preview("Sizes") {
    BrandSamples(.accountPicture)
  }
#endif
