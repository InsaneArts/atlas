import SwiftUI

/// The panel's picture: the helper's screen resting on the ground, which opens the main window on
/// Screen when clicked and shows the expand symbol under the pointer to say so. Its look tells its state: the blue rim
/// of control, greyed and darkened under an orange badge while the helper needs the person, greyed and darkened
/// alone while paused. The newest action floats over its corner on a solid chip while an agent works, orange when it
/// failed, and the solid pause circle sits in the other corner, pausing the picture only. With no screen to show, the
/// empty stage says why.
struct PanelPicture: View {
  /// What the frame shows, and how.
  let state: PanelPictureState
  /// Receives a click on the screen and on the pause button.
  let send: @MainActor (PanelEvent) -> Void
  /// Whether the pointer is over the screen, which shows the expand symbol.
  @State private var isHovered = false
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The helper's screen, or the empty stage, in a frame of the screen's shape.
  var body: some View {
    ZStack {
      if state.showsScreen {
        screen
      } else {
        PanelEmptyPicture(message: state.emptyMessage ?? "", breathes: state.breathes)
      }
    }
    .frame(width: size.width, height: size.height)
  }

  /// The frame: the screen's own shape while it shows, so no black shows at its sides, and 16:10 otherwise.
  private var size: CGSize {
    PanelLayout.pictureSize(aspect: state.showsScreen ? state.picture?.aspectRatio : nil)
  }

  /// The screen with what sits on it: the rim, the badge, the expand symbol, the caption and the pause button, dark in
  /// either appearance, as the helper's screen is framed in black.
  private var screen: some View {
    Button {
      send(.openScreen)
    } label: {
      PanelScreen(picture: state.picture, isDimmed: state.isDimmed)
    }
    .buttonStyle(.plain)
    .help("Open the helper's screen in Atlas")
    .accessibilityLabel("The helper's screen")
    .accessibilityHint("Opens it in Atlas")
    .overlay { PanelControlRim(isShown: state.showsRim) }
    .overlay(alignment: .topLeading) { badge }
    .overlay(alignment: .topTrailing) { expand }
    .overlay(alignment: .bottomLeading) { caption }
    .overlay(alignment: .bottomTrailing) { pauseButton }
    .environment(\.colorScheme, .dark)
    .onHover { isHovered = $0 }
  }

  /// The orange badge while the helper needs the person, popping in.
  private var badge: some View {
    AttentionBadge(side: PanelLayout.badge)
      .scaleEffect(state.showsBadge || reduceMotion ? 1 : PanelLayout.badgeArrival)
      .opacity(state.showsBadge ? 1 : 0)
      .animation(
        Motion.animation(Motion.pop, reduceMotion: reduceMotion, reduced: Motion.reduced),
        value: state.showsBadge
      )
      .padding(PanelLayout.captionInset)
      .allowsHitTesting(false)
  }

  /// The expand symbol under the pointer: what a click on the screen does.
  private var expand: some View {
    Image(systemName: SymbolName.expand.rawValue)
      .font(Typography.font(size: PanelLayout.cornerSymbol, weight: .semibold))
      .foregroundStyle(Color.textOverPicture)
      .frame(width: PanelLayout.pauseButton, height: PanelLayout.pauseButton)
      .background {
        Circle()
          .fill(Color.fillOverPicture)
      }
      .overlay { edge }
      .revealsOnHover(isHovered)
      .padding(PanelLayout.captionInset)
      .allowsHitTesting(false)
      .accessibilityHidden(true)
  }

  /// The newest action while an agent works, on the panel's smaller solid chip, led by its session's icon while
  /// sessions show, a new one replacing the last by a fade of 0.2 seconds.
  private var caption: some View {
    ZStack(alignment: .bottomLeading) {
      if let caption = state.caption {
        PictureChip(lead: Self.lead(caption), time: caption.time.isEmpty ? nil : caption.time, size: .small) {
          CaptionLine(icon: caption.icon, text: caption.text)
        }
        .frame(maxWidth: PanelLayout.captionWidth, alignment: .leading)
        .padding(PanelLayout.captionInset)
        .id(caption.text)
        .transition(.opacity)
      }
    }
    .animation(Motion.fade, value: state.caption?.text)
    .allowsHitTesting(false)
  }

  /// The pause button, a solid dark circle with a fine edge so it reads over any picture: pause turns into play at once
  /// once the picture is paused, each drawn centred in the circle's square, with no replace effect, which leaves the new
  /// symbol in the old one's box, off the circle's centre. It pauses the picture only; work in the helper goes on.
  private var pauseButton: some View {
    Button {
      send(.togglePause)
    } label: {
      Image(systemName: (state.pause.isPaused ? SymbolName.play : SymbolName.pause).rawValue)
        .contentTransition(.identity)
    }
    .buttonStyle(.capsule(.overPicture, size: .mini, outline: .circle))
    .overlay { edge }
    .help(state.pause.help)
    .accessibilityLabel(state.pause.label)
    .padding(PanelLayout.captionInset)
  }

  /// The fine edge round a circle over the picture, as round the chips, so it reads over a dark screen too.
  private var edge: some View {
    Circle()
      .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
      .allowsHitTesting(false)
  }

  /// What leads the caption: an orange dot when its action failed, as the viewer's captions mark a failure; else the
  /// steady blue dot of work under way, unless its session's icon leads it; the header's dot is the one that breathes.
  ///
  /// - Parameter caption: The newest action.
  /// - Returns: The chip's lead.
  nonisolated static func lead<Content: View>(_ caption: PanelCaption) -> PictureChip<Content>.Lead {
    if caption.isFailed { return .dot(.attention) }
    return caption.icon == nil ? .dot(.active) : .none
  }
}

#if DEBUG
  #Preview("Working") {
    PanelPicture(state: PreviewStates.panelWorking.picture) { _ in }
      .sampleSurface()
  }

  #Preview("Needs you") {
    PanelPicture(state: PreviewStates.panelNeedsYou.picture) { _ in }
      .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.panelIdle
    InteractivePreview {
      Menu(String("State")) {
        ForEach(PreviewStates.panel) { preview in
          Button(preview.name) { state = preview.value }
        }
      }
      .fixedSize()
    } content: {
      PanelPicture(state: state.picture) { event in
        state = PreviewStates.panel(state, applying: event)
      }
      .animation(PanelLayout.change, value: state.picture.look)
    }
  }
#endif
