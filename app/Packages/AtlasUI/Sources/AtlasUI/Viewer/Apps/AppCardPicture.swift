import SwiftUI

/// A card's picture: its whole front window cut from the helper's screen, fitted in the slot, while the window
/// is in view; else where its windows sit on the display (``AppScreenMap``). Each new frame replaces the last with no
/// animation, as the live picture does; the window's picture and the map fade into each other.
struct AppCardPicture: View {
  /// The picture.
  let picture: ViewerAppPicture

  /// The window's picture or the map, in the slot.
  var body: some View {
    ZStack {
      RoundedRectangle(cornerRadius: ViewerLayout.appCardPictureRadius, style: .continuous)
        .fill(Color.fillControl)
      if let window = picture.window {
        Color.clear
          .overlay {
            Image(decorative: window.image, scale: 1)
              .resizable()
              .interpolation(.high)
              .scaledToFit()
          }
          .transition(Self.fade)
      } else {
        AppScreenMap(map: picture.map)
          .padding(ViewerLayout.appMapInset)
          .transition(Self.fade)
      }
    }
    .frame(maxWidth: .infinity)
    .frame(height: ViewerLayout.appCardPicture)
    .clipShape(RoundedRectangle(cornerRadius: ViewerLayout.appCardPictureRadius, style: .continuous))
    .accessibilityHidden(true)
  }

  /// How the window's picture and the map fade into each other, on their own fade, so the slot never takes it and
  /// moves with its card.
  private static var fade: AnyTransition {
    AnyTransition.opacity
      .animation(AppsMotion.picture)
  }
}
