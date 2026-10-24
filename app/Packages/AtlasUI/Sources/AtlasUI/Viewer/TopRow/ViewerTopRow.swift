import SwiftUI

/// The main window's one quiet bar: what the helper is doing at the leading edge, the four tabs centred in
/// their glass track, and only the actions that apply at the trailing edge, straight on the window's ground;
/// a faint accent wash lies across it while this window holds control. A narrowing window takes the widest of
/// the bar's layouts that fits.
struct ViewerTopRow: View {
  /// What the row shows.
  let state: ViewerTopRowState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The widest layout that fits, washed in the accent while in control.
  var body: some View {
    ViewThatFits(in: .horizontal) {
      ForEach(ViewerLayout.barLayouts, id: \.self) { fit in
        row(fit)
      }
    }
    .padding(.leading, ViewerLayout.statusInset(isFullScreen: state.isFullScreen))
    .padding(.trailing, Spacing.mPlus)
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .background {
      // A colour change rides a curve, never a spring; only opacity changes, so Reduce Motion keeps it.
      Color.accentTint
        .opacity(state.isInControl ? 1 : 0)
        .animation(Motion.fade, value: state.isInControl)
    }
  }

  /// The bar in one of its layouts.
  ///
  /// - Parameter fit: The layout.
  /// - Returns: The bar.
  private func row(_ fit: ViewerTopRowFit) -> some View {
    ViewerBarArrangement(gap: ViewerLayout.columnGap) {
      ViewerTopStatus(status: state.status, reservesLine: fit.reservesLine)
        .accessibilityIdentifier("status")
      Tabs(state: state.tabs, face: fit.tabFace) { tab in
        send(.selectTab(tab))
      } accessory: { tab in
        if tab == .activity {
          ViewerUnseenDot(unseen: state.activityUnseen)
        }
      }
      ViewerTopButtons(
        actions: state.actions,
        actionWords: fit.actionWords,
        showsPause: fit.showsPause,
        send: send
      )
    }
  }
}

#if DEBUG
  #Preview("Working") {
    ViewerTopRow(state: PreviewStates.viewerWorking.top) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: ViewerLayout.topRowHeight)
      .sampleSurface()
  }

  #Preview("You have control, narrowest") {
    ViewerTopRow(state: PreviewStates.viewerControl.top) { _ in }
      .frame(width: PreviewStates.viewerNarrowest, height: ViewerLayout.topRowHeight)
      .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerWorking
    @Previewable @State var width = PreviewStates.viewerSize.width
    InteractivePreview {
      Menu(String("State")) {
        ForEach(PreviewStates.viewer) { preview in
          Button(preview.name) { state = preview.value }
        }
      }
      .fixedSize()
      Slider(value: $width, in: PreviewStates.viewerNarrowest...PreviewStates.viewerSize.width)
    } content: {
      ViewerTopRow(state: state.top) { event in
        state = PreviewStates.viewer(state, applying: event)
      }
      .frame(width: width, height: ViewerLayout.topRowHeight)
    }
  }
#endif
