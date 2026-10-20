import SwiftUI

/// The helper's desktop at its own shape, with what is happening laid over it. The shell's input surface lies over
/// the picture while there is one.
struct StageView<Input: View>: View {
  /// What the stage shows.
  let state: ViewerStageState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// The surface laid over the picture that takes the mouse and keyboard.
  let input: Input

  /// The picture with the input surface, or why there is none; the chips, notice and captions over it.
  var body: some View {
    ZStack {
      if let picture = state.picture {
        Color.stageBackground
        Image(picture.image, scale: 1, label: Text("The helper's screen"))
          .resizable()
          .interpolation(.high)
          .aspectRatio(contentMode: .fit)
          .opacity(state.isDimmed ? ViewerLayout.pausedPicture : 1)
        input
      } else {
        Color.fillCard
        if let placeholder = state.placeholder {
          StagePlaceholder(placeholder: placeholder, send: send)
        }
      }
      VStack(spacing: Spacing.m) {
        chip
        if let notice = state.notice {
          FloatingChip {
            Text(notice)
          }
          .transition(.opacity)
        }
        Spacer()
        HStack(alignment: .bottom) {
          CaptionList(entries: state.captions)
          Spacer()
        }
      }
      .padding(Spacing.xl)
      .allowsHitTesting(false)
    }
    .aspectRatio(aspect, contentMode: .fit)
    .clipShape(RoundedRectangle(cornerRadius: Radius.large))
    .overlay {
      RoundedRectangle(cornerRadius: Radius.large)
        .strokeBorder(edge, lineWidth: edgeWidth)
    }
    .shadow(color: glow, radius: ViewerLayout.controlGlowRadius)
    .elevation(.stage)
    .animation(Motion.fade, value: state.notice)
    .animation(Motion.fade, value: state.isInControl)
    .accessibilityElement(children: .contain)
    .accessibilityLabel("Helper desktop")
  }

  /// The stage's shape: the picture's, or the placeholder's before one arrives.
  private var aspect: CGSize {
    guard let picture = state.picture else { return ViewerLayout.placeholderAspect }
    return CGSize(width: picture.image.width, height: picture.image.height)
  }

  /// The stage's edge: the accent while this window holds control, a quiet line otherwise.
  private var edge: Color {
    guard state.isInControl else { return .separator }
    return Color.accent
      .opacity(ViewerLayout.controlEdge)
  }

  /// The edge's width: firmer while in control.
  private var edgeWidth: CGFloat {
    state.isInControl ? Stroke.emphasis : Stroke.hairline
  }

  /// The accent glowing around the stage while in control; none otherwise.
  private var glow: Color {
    guard state.isInControl else { return .clear }
    return Color.accent
      .opacity(ViewerLayout.controlGlow)
  }

  /// The chip at the top: what is being typed, control, or a paused picture.
  @ViewBuilder private var chip: some View {
    switch state.chip {
    case .typing(let text):
      FloatingChip {
        Text("Typing")
          .foregroundStyle(Color.textTertiary)
        Text(text)
          .underline()
      }
    case .inControl:
      FloatingChip {
        StatusDot(tone: .active)
        Text("You have control")
        Text("Control-Shift-Escape gives it back")
          .foregroundStyle(Color.textTertiary)
      }
    case .paused:
      FloatingChip {
        Image(systemName: "pause.fill")
          .font(Typography.font(.caption2))
          .foregroundStyle(Color.textSecondary)
        Text("Picture paused")
      }
    case nil:
      EmptyView()
    }
  }
}

#if DEBUG
  #Preview("Typing") {
    StageView(state: PreviewStates.viewerTyping.stage, send: { _ in }, input: EmptyView())
      .frame(width: SampleWidth.column * 2)
      .sampleSurface()
  }
#endif
