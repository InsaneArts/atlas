import SwiftUI

/// The picture sheet over the window: the window dims behind it, and the sheet hangs from under the card's header,
/// sliding down into place and back up as it closes. Pressing the dimmed window, Done, Return or Escape closes it.
struct PictureSheetLayer: View {
  /// The sheet; nil shows none.
  let sheet: ViewerPictureSheet?
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.motionReduced) private var reduceMotion

  /// The scrim and the sheet.
  var body: some View {
    GeometryReader { proxy in
      ZStack(alignment: .top) {
        if let sheet {
          Color.scrim
            .contentShape(Rectangle())
            .onTapGesture { send(.closePicture) }
            .transition(.opacity)
            .accessibilityHidden(true)
          PictureSheetCard(sheet: sheet, window: proxy.size, send: send)
            .padding(.top, ViewerLayout.sheetTop)
            .transition(hanging)
        }
      }
      .frame(width: proxy.size.width, height: proxy.size.height, alignment: .top)
    }
    .animation(
      Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.crossfadeReduced),
      value: sheet == nil
    )
    .accessibilityElement(children: .contain)
    .accessibilityAddTraits(sheet == nil ? [] : .isModal)
  }

  /// How the sheet comes and goes: down from under the header and back up, fading; only fading under Reduce Motion.
  private var hanging: AnyTransition {
    guard !reduceMotion else { return .opacity }
    return AnyTransition.opacity
      .combined(with: .offset(y: -ViewerLayout.sheetDrop))
  }
}
