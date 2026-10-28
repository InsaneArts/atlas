import SwiftUI

/// The header over one host session's lines while Activity shows every session's work, pinned as the list scrolls:
/// the host's icon, or the person's blue dot for their own lines, the session's name, its state in its tone's colour,
/// and Show all at the trailing edge while the session holds more lines than it lists.
struct ActivitySessionHeader: View {
  /// The group it heads.
  let group: ViewerActivitySessionGroup
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The icon, the name and the state, and Show all, on the pinned ground.
  var body: some View {
    HStack(spacing: Spacing.s) {
      lead
      Text(group.name)
        .font(Typography.font(.callout, weight: .semibold))
        .foregroundStyle(Color.textPrimary)
        .truncationMode(.tail)
        .accessibilityAddTraits(.isHeader)
      if !group.state.isEmpty {
        Text(group.state)
          .font(Typography.font(.callout, weight: .medium))
          .foregroundStyle(group.tone.stateColor)
          .fixedSize()
      }
      Spacer(minLength: .zero)
      if let more = group.more, let showAll = group.showAll {
        Button(more) { send(showAll) }
          .buttonStyle(.textLink)
          .fixedSize()
          .accessibilityLabel(group.moreLabel.isEmpty ? more : group.moreLabel)
      }
    }
    .lineLimit(1)
    .padding(.top, Spacing.sPlus)
    .padding(.bottom, Spacing.xsPlus)
    .padding(.horizontal, Spacing.l)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(Color.fillPinned)
  }

  /// The host's icon, or the person's blue dot in the icon's place.
  @ViewBuilder private var lead: some View {
    if let icon = group.icon {
      SessionIcon(icon: icon, side: ViewerLayout.groupIcon)
    } else {
      StatusDot(mark: .dot, tone: .active, size: .small)
        .frame(width: ViewerLayout.groupIcon, height: ViewerLayout.groupIcon)
        .accessibilityHidden(true)
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    if case .activity(let activity) = PreviewStates.viewerActivityNeedsYou.pane {
      VStack(spacing: .zero) {
        ForEach(activity.sessions) { group in
          ActivitySessionHeader(group: group) { _ in }
        }
      }
      .frame(width: SampleWidth.column * 2)
      .sampleSurface()
    }
  }

  #Preview("Interactive") {
    @Previewable @State var chosen = String("Nothing chosen")
    if case .activity(let activity) = PreviewStates.viewerActivityBySession.pane {
      InteractivePreview {
        Text(chosen)
      } content: {
        VStack(spacing: .zero) {
          ForEach(activity.sessions) { group in
            ActivitySessionHeader(group: group) { event in chosen = String(describing: event) }
          }
        }
        .frame(width: SampleWidth.column * 2)
      }
    }
  }
#endif
