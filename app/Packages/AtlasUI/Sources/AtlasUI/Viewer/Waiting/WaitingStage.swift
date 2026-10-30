import SwiftUI

/// The stage while there is no picture: Not ready, Logged out, Connecting, or a picture paused before the
/// first, each as one pattern of mark, title, line and button on the empty stage. Connecting again keeps the
/// last picture behind its words, blurred and dimmed, so nothing goes black, and rings the mark once it has
/// waited a moment. The empty stage follows the Mac's appearance; with the last picture behind it, it stays dark, as
/// the helper's screen is framed in black in either.
struct WaitingStage: View {
  /// Why there is no picture.
  let empty: ViewerEmpty
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The message on the empty stage, the last picture behind it, in the dark, while connecting again.
  var body: some View {
    if let picture = empty.lastPicture {
      stage(behind: picture)
        .environment(\.colorScheme, .dark)
    } else {
      stage(behind: nil)
    }
  }

  /// The message on the stage's ground, over the last picture when there is one.
  ///
  /// - Parameter picture: The last picture shown, or nil for none.
  /// - Returns: The stage.
  private func stage(behind picture: DesktopPicture?) -> some View {
    EmptyMessage(state: empty.message, size: .stage, ground: .pane, tracesMark: empty.showsRing) {
      if let event = empty.event { send(event) }
    }
    .background {
      ZStack {
        shape
          .fill(Color.fillPlaceholder)
        if let picture {
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
    RoundedRectangle(cornerRadius: ViewerLayout.paneRadius, style: .continuous)
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
