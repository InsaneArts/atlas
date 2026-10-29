import SwiftUI

/// The panel's picture: the helper's screen resting on the ground, which opens the main window on
/// Screen when clicked and shows the expand symbol under the pointer to say so. Its look tells its state: the blue rim
/// of control, greyed and darkened under an orange badge while the helper needs the person, greyed and darkened
/// alone while paused. The newest action floats over its corner on glass while an agent works, orange when it failed,
/// and the pause button sits in the other corner, pausing the picture only. With no screen to show, the empty stage
/// says why.
struct PanelPicture: View {
  /// What the frame shows, and how.
  let state: PanelPictureState
  /// Receives a click on the screen and on the pause button.
  let send: @MainActor (PanelEvent) -> Void
  /// Whether the pointer is over the screen, which shows the expand symbol.
  @State private var isHovered = false
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The helper's screen, or the empty stage, at the panel's picture size.
  var body: some View {
    ZStack {
      if state.showsScreen {
        screen
      } else {
        PanelEmptyPicture(message: state.emptyMessage ?? "", breathes: state.breathes)
      }
    }
    .frame(width: PanelLayout.picture.width, height: PanelLayout.picture.height)
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
      .foregroundStyle(Color.textPrimary)
      .frame(width: PanelLayout.pauseButton, height: PanelLayout.pauseButton)
      .glass(.regular, in: Circle())
      .overlay(
        Circle()
          .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
      )
      .revealsOnHover(isHovered)
      .padding(PanelLayout.captionInset)
      .allowsHitTesting(false)
      .accessibilityHidden(true)
  }

  /// The newest action on glass while an agent works, on the panel's smaller chip, led by its session's icon while
  /// sessions show, a new one replacing the last by a fade of 0.2 seconds.
  private var caption: some View {
    ZStack(alignment: .bottomLeading) {
      if let caption = state.caption {
        GlassChip(lead: Self.lead(caption), time: caption.time.isEmpty ? nil : caption.time, size: .small) {
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

  /// The pause button: pause turns into play by a symbol replace once the picture is paused. It pauses the
  /// picture only; work in the helper goes on.
  private var pauseButton: some View {
    Button {
      send(.togglePause)
    } label: {
      Image(systemName: (state.pause.isPaused ? SymbolName.play : SymbolName.pause).rawValue)
        .contentTransition(.symbolEffect(.replace))
        .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion), value: state.pause.isPaused)
    }
    .buttonStyle(.capsule(.glass, size: .mini, outline: .circle, backing: .glass))
    .help(state.pause.help)
    .accessibilityLabel(state.pause.label)
    .padding(PanelLayout.captionInset)
  }

  /// What leads the caption: an orange dot when its action failed, as the viewer's captions mark a failure; else the
  /// steady blue dot of work under way, unless its session's icon leads it; the header's dot is the one that breathes.
  ///
  /// - Parameter caption: The newest action.
  /// - Returns: The chip's lead.
  nonisolated static func lead<Content: View>(_ caption: PanelCaption) -> GlassChip<Content>.Lead {
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
