import SwiftUI

/// A replay's frames in a row under its picture, each whole and marked with its number under it, or "After"; the frame
/// shown ringed in blue. Where the frames span more than one app, each app's name stands over its run of frames, on a
/// rule. The row is lazy: a frame asks for its thumbnail as it scrolls into view, and again each time it comes back, so
/// one let go meanwhile is read again. A frame pressed shows it.
struct ReplayStrip: View {
  /// The replay.
  let replay: ViewerReplayState
  /// How wide each frame stands.
  let width: CGFloat
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The frames, scrolling across when more stand than fit.
  var body: some View {
    let groups = replay.appGroups
    ScrollViewReader { scroller in
      ScrollView(.horizontal) {
        LazyHStack(alignment: .top, spacing: Spacing.s) {
          ForEach(Array(replay.frames.enumerated()), id: \.element.id) { index, frame in
            VStack(alignment: .leading, spacing: Spacing.xs) {
              if let group = groups.first(where: { $0.frames.contains(index) }) {
                label(group, at: index)
              }
              thumbnail(frame, at: index)
            }
            .id(frame.id)
          }
        }
        .padding(ViewerLayout.replayStripInset)
      }
      .scrollIndicators(.never)
      // A lazy row would take every height offered; the strip stands as tall as its frames.
      .fixedSize(horizontal: false, vertical: true)
      .onChange(of: replay.index, initial: true) { _, index in
        guard replay.frames.indices.contains(index) else { return }
        scroller.scrollTo(replay.frames[index].id)
      }
    }
    .accessibilityElement(children: .contain)
    .accessibilityLabel("Steps")
  }

  /// A frame's piece of its app's label: the rule over it, reaching across the gap to the next frame of the same run,
  /// and over the run's first frame the app's name, as wide as the run.
  ///
  /// - Parameters:
  ///   - group: The frame's run.
  ///   - index: The frame's place.
  /// - Returns: The label's piece.
  private func label(_ group: ViewerReplayAppGroup, at index: Int) -> some View {
    let count = CGFloat(group.frames.count)
    let continues = index < group.frames.upperBound - 1
    return Rectangle()
      .fill(Color.separator)
      .frame(width: width + (continues ? Spacing.s : .zero), height: Stroke.emphasis)
      .frame(width: width, height: ViewerLayout.replayGroupLabel, alignment: .topLeading)
      .overlay(alignment: .topLeading) {
        if index == group.frames.lowerBound {
          Text(verbatim: group.app)
            .font(Typography.font(.caption, weight: .semibold))
            .foregroundStyle(Color.textTertiary)
            .lineLimit(1)
            .padding(.top, Spacing.xs)
            .frame(width: count * width + (count - 1) * Spacing.s, alignment: .leading)
            .accessibilityAddTraits(.isHeader)
        }
      }
  }

  /// One frame, its number under it.
  ///
  /// - Parameters:
  ///   - frame: The frame.
  ///   - index: Its place.
  /// - Returns: The frame's button.
  private func thumbnail(_ frame: ViewerReplayFrame, at index: Int) -> some View {
    let isShown = index == replay.index
    return Button {
      send(.replayFrame(index))
    } label: {
      VStack(spacing: Spacing.xs) {
        ActivityStepPicture(
          picture: ViewerStepPicture(shot: frame.picture, mark: frame.mark, repeats: frame.repeats),
          zooms: false,
          width: width
        )
        .overlay {
          if isShown {
            RoundedRectangle(cornerRadius: ViewerLayout.stepPictureRadius, style: .continuous)
              .strokeBorder(Color.accent, lineWidth: Stroke.thick)
          }
        }
        Text(verbatim: frame.label)
          .font(Typography.digits(.caption, weight: isShown ? .semibold : .regular))
          .foregroundStyle(isShown ? Color.accent : Color.textTertiary)
      }
    }
    .buttonStyle(.plain)
    .asksForThumbnail(frame.picture.name, send: send)
    .accessibilityLabel(frame.words.text)
    .accessibilityAddTraits(isShown ? .isSelected : [])
  }
}
