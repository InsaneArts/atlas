import SwiftUI

/// A task replayed over the whole window, on a floating card over a shade that closes it: the task's title and Done,
/// one frame at a time, each the step's marked picture before it, then the screen after the last step; what the step
/// did under it, the frames in a strip, and the controls. Frames crossfade as they change; under Reduce Motion they
/// swap at once.
struct ReplayLayer: View {
  /// The replay; nil shows none.
  let replay: ViewerReplayState?
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.motionReduced) private var reduceMotion

  /// The shade, then the card.
  var body: some View {
    GeometryReader { proxy in
      ZStack {
        if let replay {
          Color.letterbox
            .opacity(ViewerLayout.shotScrim)
            .contentShape(Rectangle())
            .onTapGesture { send(.closeReplay) }
          card(replay, in: proxy.size)
        }
      }
      .frame(width: proxy.size.width, height: proxy.size.height)
      .transition(.opacity)
    }
    .animation(ViewerLayout.shotFade, value: replay == nil)
    .accessibilityElement(children: .contain)
    .accessibilityAddTraits(replay == nil ? [] : .isModal)
  }

  /// The card: the head, the picture, what the step did, the strip and the controls.
  ///
  /// - Parameters:
  ///   - replay: The replay.
  ///   - window: The window's size.
  /// - Returns: The card.
  private func card(_ replay: ViewerReplayState, in window: CGSize) -> some View {
    let aspect = replay.frame?.picture.thumbnail.map { CGSize(width: $0.image.width, height: $0.image.height) }
    let size = ViewerLayout.replayPictureSize(
      aspect: aspect ?? ViewerLayout.placeholderAspect,
      in: window,
      labeled: !replay.appGroups.isEmpty
    )
    let thumbnail = ViewerLayout.replayFrameWidth(picture: size.width)
    return VStack(alignment: .leading, spacing: Spacing.m) {
      head(replay)
        .frame(width: size.width)
      stage(replay)
        .frame(width: size.width, height: size.height)
      say(replay)
        .frame(width: size.width, alignment: .leading)
      ReplayStrip(replay: replay, width: thumbnail, send: send)
        .frame(width: size.width)
      ReplayControls(replay: replay, send: send)
        .frame(width: size.width)
    }
    .padding(Spacing.mPlus)
    .surface(.floating, in: RoundedRectangle(cornerRadius: Radius.extraLarge, style: .continuous))
    .elevation(.modal)
  }

  /// The agent's icon, the task's title, who did it and when, and Done.
  ///
  /// - Parameter replay: The replay.
  /// - Returns: The head.
  private func head(_ replay: ViewerReplayState) -> some View {
    HStack(spacing: Spacing.sPlus) {
      if let icon = replay.icon {
        SessionIcon(icon: icon, side: ViewerLayout.replayAgentIcon)
      }
      VStack(alignment: .leading, spacing: Stroke.regular) {
        Text(verbatim: replay.title)
          .font(Typography.font(.heading, weight: .semibold))
          .foregroundStyle(Color.textPrimary)
        Text(verbatim: replay.detail)
          .font(Typography.digits(.callout))
          .foregroundStyle(Color.textSecondary)
      }
      .lineLimit(1)
      .accessibilityElement(children: .combine)
      .accessibilityAddTraits(.isHeader)
      Spacer(minLength: Spacing.s)
      Button(String("Done")) { send(.closeReplay) }
        .buttonStyle(.capsule(.secondary, size: .mini))
        .help("Back to the activity")
    }
  }

  /// The frame shown, its mark over it, crossfading as it changes.
  ///
  /// - Parameter replay: The replay.
  /// - Returns: The picture.
  private func stage(_ replay: ViewerReplayState) -> some View {
    ZStack {
      if let frame = replay.frame {
        MarkedPicture(picture: frame.picture.thumbnail, mark: frame.mark)
          .id(frame.id)
          .transition(.opacity)
          .accessibilityElement()
          .accessibilityLabel(frame.words.text)
          .accessibilityAddTraits(.isImage)
      }
    }
    .animation(Motion.animation(Motion.crossfade, reduceMotion: reduceMotion), value: replay.frame?.id)
    .clipShape(RoundedRectangle(cornerRadius: Radius.large, style: .continuous))
    .overlay {
      RoundedRectangle(cornerRadius: Radius.large, style: .continuous)
        .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
    }
  }

  /// What the frame's step did, its disc beside it, and when and where under it.
  ///
  /// - Parameter replay: The replay.
  /// - Returns: The line.
  private func say(_ replay: ViewerReplayState) -> some View {
    HStack(spacing: Spacing.sPlus) {
      if let frame = replay.frame {
        if let kind = frame.kind {
          ActivityStepDisc(kind: kind, mark: frame.stepMark)
        }
        VStack(alignment: .leading, spacing: Stroke.regular) {
          ActivityStepWords(words: frame.words)
          Text(verbatim: frame.detail)
            .font(Typography.digits(.callout))
            .foregroundStyle(Color.textSecondary)
            .lineLimit(1)
        }
      }
    }
    .frame(minHeight: ViewerLayout.replaySay, alignment: .leading)
    .accessibilityElement(children: .combine)
    .accessibilityAddTraits(.updatesFrequently)
  }
}

#if DEBUG
  #Preview("Replay") {
    ReplayLayer(replay: PreviewStates.replay) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
      .windowSurface()
  }

  #Preview("Replay, after, dark") {
    var replay = PreviewStates.replay
    replay.index = replay.frames.count - 1
    return ReplayLayer(replay: replay) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
      .windowSurface()
      .environment(\.colorScheme, .dark)
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerActivity
    InteractivePreview {
      Button(String("Replay Opie")) { state = PreviewStates.viewer(state, applying: .openReplay("session:|o1")) }
    } content: {
      ViewerView(state: state) { event in
        state = PreviewStates.viewer(state, applying: event)
      }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
    }
  }
#endif
