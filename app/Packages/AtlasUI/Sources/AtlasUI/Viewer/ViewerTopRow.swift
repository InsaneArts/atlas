import SwiftUI

/// The main window's one quiet row: the helper's status after the window buttons, the tabs in the middle and the
/// buttons at the end, straight on the window's ground with no bar, line or shadow of its own.
struct ViewerTopRow: View {
  /// What the row shows.
  let state: ViewerTopRowState
  /// How far the row draws in for the window's width.
  let fit: ViewerTopRowFit
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  var body: some View {
    HStack(spacing: ViewerLayout.columnGap) {
      StatusLabel(status: state.status, showsLine: fit < .statusWord)
        .padding(.leading, ViewerLayout.statusInset(isFullScreen: state.isFullScreen))
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityIdentifier("status")
      CapsuleTabs(state: state.tabs, size: fit < .tightTabs ? .regular : .compact) { send(.selectTab($0)) }
      ViewerTopButtons(actions: state.actions, fit: fit, send: send)
        .frame(maxWidth: .infinity, alignment: .trailing)
    }
    .padding(.trailing, Spacing.mPlus)
  }
}

#if DEBUG
  #Preview("Working") {
    ViewerTopRow(state: PreviewStates.viewerWorking.top, fit: .regular) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: ViewerLayout.topRowHeight)
      .sampleSurface()
  }

  #Preview("You have control, narrowest") {
    ViewerTopRow(state: PreviewStates.viewerControl.top, fit: .shortWords) { _ in }
      .frame(width: ViewerLayout.longWordsRoom - Spacing.xxxl, height: ViewerLayout.topRowHeight)
      .sampleSurface()
  }
#endif
