import SwiftUI

/// What lies over the screen: the chip and the notice at the top, which take no clicks, and the latest actions piled
/// in the bottom corner, which fan out under the pointer. One chip replaces another in the same place by a fade
/// of 0.2 seconds, as the typing chip takes control's; a notice fades in and out.
struct ScreenOverlay: View {
  /// What the screen shows.
  let state: ViewerScreenState
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The chip and the notice at the top, the captions in the corner.
  var body: some View {
    ZStack(alignment: .bottomLeading) {
      top
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .allowsHitTesting(false)
      // The pile takes the pointer, to fan out; while it is empty nothing over the picture does.
      CaptionStack(entries: state.captions)
        .allowsHitTesting(!state.captions.isEmpty)
        .padding([.horizontal, .bottom], Spacing.mPlus)
    }
  }

  /// The chip and, under it, the notice.
  private var top: some View {
    VStack(spacing: Spacing.s) {
      ZStack {
        if let chip = state.chip {
          ScreenChip(chip: chip)
            .id(Self.slot(chip))
            .transition(.opacity)
        }
      }
      .animation(Motion.fade, value: state.chip.map(Self.slot))
      if let notice = state.notice {
        FloatingChip(lines: ViewerLayout.noticeLines) {
          Text(notice)
        }
        .frame(maxWidth: ViewerLayout.noticeWidth)
        .transition(.opacity)
      }
    }
    .padding(.top, Spacing.m)
    .padding(.horizontal, Spacing.mPlus)
    .animation(Motion.animation(Motion.fade, reduceMotion: reduceMotion, reduced: Motion.reduced), value: state.notice)
  }

  /// Which chip stands in the slot, whatever it says: a new one replaces it, new words in the same one do not.
  ///
  /// - Parameter chip: The chip.
  /// - Returns: Its kind.
  nonisolated static func slot(_ chip: ViewerScreenChip) -> Int {
    switch chip {
    case .typing: 0
    case .inControl: 1
    case .paused: 2
    case .holder: 3
    }
  }
}

#if DEBUG
  #Preview("Working") {
    ScreenOverlay(state: PreviewStates.viewerWorking.screen)
      .frame(width: SampleWidth.column * 2, height: SampleWidth.column)
      .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var chip = ViewerScreenChip.inControl
    InteractivePreview {
      Button(String("Type")) { chip = .typing("こんにちは") }
      Button(String("Control")) { chip = .inControl }
      Button(String("Paused")) { chip = .paused }
      Button(String("Holder")) { chip = .holder(name: SessionSamples.claudeName, icon: SessionSamples.claudeIcon) }
    } content: {
      ScreenOverlay(state: ViewerScreenState(chip: chip))
        .frame(width: SampleWidth.column * 2, height: SampleWidth.column)
    }
  }
#endif
