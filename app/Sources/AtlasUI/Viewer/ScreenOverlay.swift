import SwiftUI

/// What lies over the screen and takes no clicks: the chip and the notice at the top, the latest actions in the
/// bottom corner. A notice fades in and out, or comes and goes at once under Reduce Motion.
struct ScreenOverlay: View {
  /// What the screen shows.
  let state: ViewerScreenState
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    VStack(spacing: Spacing.m) {
      if let chip = state.chip {
        ScreenChip(chip: chip)
      }
      if let notice = state.notice {
        FloatingChip(lines: ViewerLayout.noticeLines) {
          Text(notice)
        }
        .frame(maxWidth: ViewerLayout.noticeWidth)
        .transition(reduceMotion ? .identity : .opacity)
      }
      Spacer(minLength: .zero)
      CaptionStack(entries: state.captions)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
    .padding(Spacing.xl)
    .animation(reduceMotion ? nil : Motion.fade, value: state.notice)
    .allowsHitTesting(false)
  }
}

#if DEBUG
  #Preview("Working") {
    ScreenOverlay(state: PreviewStates.viewerWorking.screen)
      .frame(width: SampleWidth.column * 2, height: SampleWidth.column)
      .sampleSurface()
  }
#endif
