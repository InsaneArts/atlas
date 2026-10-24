import SwiftUI

/// The panel's picture: the helper's screen, which opens the main window on Screen when clicked, with the newest
/// action over its corner while an agent works; or, with no screen to show, the empty stage saying why.
struct PanelPicture: View {
  /// What fills the frame.
  let state: PanelPictureState
  /// Receives a click on the screen.
  let send: @MainActor (PanelEvent) -> Void

  var body: some View {
    switch state {
    case .screen(let picture, let look, let caption):
      Button {
        send(.openScreen)
      } label: {
        PictureFrame(picture: picture, look: look, size: .small)
          .overlay(alignment: .bottomLeading) {
            if let caption {
              CaptionStack(entries: [caption])
                .padding(PanelLayout.captionInset)
            }
          }
      }
      .buttonStyle(.plain)
      .frame(width: PanelLayout.picture.width, height: PanelLayout.picture.height)
      .elevation(.raised)
      .help("Open the helper's screen in Atlas")
      .accessibilityLabel("The helper's screen")
      .accessibilityHint("Opens it in Atlas")
    case .empty(let line):
      PanelEmptyPicture(line: line)
        .frame(width: PanelLayout.picture.width, height: PanelLayout.picture.height)
    }
  }
}

#if DEBUG
  #Preview("Working") {
    PanelPicture(state: PreviewStates.panelWorking.picture) { _ in }
      .sampleSurface()
  }

  #Preview("Logged out") {
    PanelPicture(state: PreviewStates.panelLoggedOut.picture) { _ in }
      .sampleSurface()
  }
#endif
