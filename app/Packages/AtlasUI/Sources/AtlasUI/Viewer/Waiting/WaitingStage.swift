import SwiftUI

/// The stage while there is no picture (v3 V7): Not ready, Logged out, Connecting, or a picture paused before the
/// first, each as one pattern of mark, title, line and button on the empty stage. Connecting again keeps the last
/// picture behind its words, blurred and dimmed, so nothing goes black (v3 R8, X4), and rings the mark once it has
/// waited a moment (R12).
struct WaitingStage: View {
  /// Why there is no picture.
  let empty: ViewerEmpty
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The message on the empty stage, the last picture behind it while connecting again.
  var body: some View {
    EmptyMessage(state: empty.message, size: .stage, ground: .pane, tracesMark: empty.showsRing) {
      if let event = empty.event { send(event) }
    }
    .background {
      ZStack {
        shape
          .fill(Color.fillPlaceholder)
        if let picture = empty.lastPicture {
          WaitingBackdrop(picture: picture)
        }
      }
      .clipShape(shape)
      .overlay(
        shape
          .strokeBorder(Color.separator, lineWidth: Stroke.hairline)
      )
    }
  }

  /// The stage's outline.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: Radius.large)
  }
}

#if DEBUG
  #Preview("Connecting again") {
    if let empty = PreviewStates.viewerConnecting.screen.empty {
      WaitingStage(empty: empty) { _ in }
        .frame(width: SampleWidth.column * 2, height: SampleWidth.column)
        .sampleSurface()
    }
  }

  #Preview("Interactive") {
    @Previewable @State var showsRing = false
    @Previewable @State var keepsPicture = true
    InteractivePreview {
      Toggle(String("Ring"), isOn: $showsRing)
      Toggle(String("Last picture"), isOn: $keepsPicture)
    } content: {
      WaitingStage(
        empty: ViewerEmpty(
          message: EmptyMessageState(headline: String("Reconnecting…"), message: String()),
          lastPicture: keepsPicture ? SampleDesktop.picture : nil,
          showsRing: showsRing
        )
      ) { _ in }
      .frame(width: SampleWidth.column * 2, height: SampleWidth.column)
    }
  }
#endif
