import SwiftUI

/// One step of an open moment: its disc on the moment's line under the moment's icon, what was done from the
/// moment's title's edge, and when at the end; a soft capsule under the pointer, reaching past the words on each side
/// so nothing moves. A screenshot's step offers Show large under the pointer and shows its thumbnail under its words;
/// a step that did not work holds its words in a callout that says why and what came next. It reads as one, with its
/// screenshot's Show large, or Show Screen and Recover…, as its actions.
struct ActivityStepRow: View {
  /// The step.
  let step: ViewerActivityStep
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// Whether the pointer is over the step's line.
  @State private var isHovered = false
  @Environment(\.hoverPaused) private var hoverPaused

  /// The step's line, then its thumbnail, on the moment's line.
  var body: some View {
    VStack(alignment: .leading, spacing: .zero) {
      line
      if let shot = step.shot {
        thumbnail(shot)
          .background(alignment: .leading) {
            if !step.isLast {
              ActivityTimelineSegment()
            }
          }
      }
    }
    .accessibilityElement(children: .combine)
    .accessibilityLabel(step.label)
    .accessibilityActions {
      if let shot = step.shot {
        Button("Show large") { send(.openShot(shot.name)) }
      }
      if step.offersRecovery {
        Button("Show Screen") { send(.show(.screen)) }
        Button("Recover…") { send(.askRecover) }
      }
    }
  }

  /// The disc, the words or the callout, Show large, and the time.
  private var line: some View {
    HStack(spacing: .zero) {
      ActivityStepDisc(kind: step.kind, mark: step.mark)
        .frame(width: ViewerLayout.appIcon)
        .frame(width: ViewerLayout.stepLeading, alignment: .leading)
      HStack(spacing: Spacing.s) {
        if step.problem != nil {
          ActivityProblemCallout(step: step, send: send)
            .padding(.vertical, Spacing.half)
        } else {
          ActivityStepWords(words: step.words, isLive: step.mark == .live)
        }
        if let shot = step.shot {
          Button("Show large") { send(.openShot(shot.name)) }
            .buttonStyle(.textLink)
            .fixedSize()
            .revealsOnHover(isHovered && !hoverPaused)
            .help("Show this screenshot large")
        }
        Spacer(minLength: Spacing.s)
        Text(verbatim: step.time)
          .font(Typography.digits(.callout))
          .foregroundStyle(Color.textTertiary)
          .contentTransition(.identity)
          .fixedSize()
      }
      .frame(minHeight: ViewerLayout.stepHeight)
      .padding(.horizontal, ViewerLayout.capsuleReach)
      .background(
        // Only the capsule's opacity takes the hover's fade, so the row moves as one when the list moves it.
        Capsule()
          .fill(Color.fillRowHover)
          .animation(Motion.hover(isHovered)) { capsule in
            capsule.opacity(isHovered && !hoverPaused && step.problem == nil ? 1 : 0)
          }
      )
      .padding(.horizontal, -ViewerLayout.capsuleReach)
    }
    .background(alignment: .leading) {
      ActivityTimelineSegment(endsHere: step.isLast)
    }
    .contentShape(Rectangle())
    .onHover { isHovered = $0 }
  }

  /// The whole screenshot at its own shape, which shows it large: 132 across, or as tall as a 4:3 one where it is
  /// narrower, standing at the leading edge of its room.
  ///
  /// - Parameter shot: The screenshot.
  /// - Returns: The thumbnail.
  private func thumbnail(_ shot: ViewerActivityShot) -> some View {
    Button {
      send(.openShot(shot.name))
    } label: {
      ShotThumbnail(
        picture: shot.thumbnail,
        shape: shot.shape,
        radius: ViewerLayout.stepThumbnailRadius,
        showsEdge: false,
        ground: .fillControl,
        alignment: .leading
      )
      // Its room keeps the list's shape for it, so it never resizes as the thumbnail arrives.
      .frame(width: ViewerLayout.stepThumbnail, height: ViewerLayout.stepThumbnail / clamped(shot.aspect))
    }
    .buttonStyle(.plain)
    .help("Show this screenshot large")
    .asksForThumbnail(shot.name, send: send)
    .padding(.top, Spacing.xs)
    .padding(.bottom, ViewerLayout.stepInset)
    .padding(.leading, ViewerLayout.stepLeading)
  }

  /// A screenshot's shape kept between 4:3 and 16:9.
  ///
  /// - Parameter aspect: Its width over its height.
  /// - Returns: The shape the thumbnail takes.
  private func clamped(_ aspect: CGFloat) -> CGFloat {
    min(max(aspect, ViewerLayout.shotAspects.lowerBound), ViewerLayout.shotAspects.upperBound)
  }
}

#if DEBUG
  #Preview("Steps") {
    ActivitySamples(.steps)
  }

  #Preview("Interactive") {
    ActivitySamples(.arriving)
  }
#endif
