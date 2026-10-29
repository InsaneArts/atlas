import SwiftUI

/// The chosen host session's groups in Details, before the helper's: Session (its host, folder, start and when it was
/// last seen or ended), the apps it opened, the copies Atlas made for it, the environment plans it applied, and its
/// newest screenshots as thumbnails, each opening large, with See all for the rest on the Screenshots tab; last, what
/// its last cleanup did and Clean Up This Session's Work…, while it left something to clean up. A group with nothing
/// in it is left out. Only the cleanup acts in the helper, and only once the person confirms it.
struct SessionDetailGroups: View {
  /// The session.
  let detail: ViewerSessionDetail
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The header, then each group that has rows, then the cleanup.
  var body: some View {
    VStack(alignment: .leading, spacing: ViewerLayout.detailsInset) {
      SessionDetailHeader(detail: detail)
      group("Session", detail.facts)
      group("Apps", detail.apps)
      group("Copies", detail.copies)
      group("Environment plans", detail.plans)
      if !detail.shots.isEmpty {
        shots
      }
      if let cleanup = detail.cleanup {
        if !cleanup.receipt.isEmpty {
          CleanupReceipt(steps: cleanup.receipt)
        }
        SessionCleanupButton(cleanup: cleanup, send: send)
      }
    }
  }

  /// A group of facts, left out while it has none. Facts may share a name, as two plans made in one minute do, so
  /// each is told apart by its place.
  ///
  /// - Parameters:
  ///   - title: The group's name.
  ///   - facts: Its rows.
  /// - Returns: The group.
  @ViewBuilder private func group(_ title: String, _ facts: [Fact]) -> some View {
    if !facts.isEmpty {
      FactGroup(title) {
        ForEach(Array(facts.enumerated()), id: \.offset) { _, fact in
          FactRow(fact.name, value: fact.value)
        }
      }
    }
  }

  /// The newest screenshots, each opening large, with See all beside the group's name.
  private var shots: some View {
    VStack(alignment: .leading, spacing: Spacing.xs) {
      HStack(spacing: Spacing.s) {
        GroupLabel(text: "Screenshots")
        Spacer(minLength: .zero)
        if let all = detail.allShots {
          Button(all) { send(.selectTab(.screenshots)) }
            .buttonStyle(.textLink(.small))
            .help("Show this session's screenshots")
        }
      }
      HStack(spacing: Spacing.s) {
        ForEach(detail.shots, id: \.name) { shot in
          Button {
            send(.openShot(shot.name))
          } label: {
            PictureTile(picture: shot.thumbnail, radius: Radius.extraSmall)
              .frame(width: ViewerLayout.detailShot.width, height: ViewerLayout.detailShot.height)
          }
          .buttonStyle(.plain)
          .help("Show this screenshot large")
          .accessibilityLabel(shot.label)
          .asksForThumbnail(shot.name, send: send)
        }
      }
    }
    .accessibilityElement(children: .contain)
  }
}

#if DEBUG
  #Preview("Samples") {
    if let detail = PreviewStates.viewerSessionDetail.details?.hostSession {
      SessionDetailGroups(detail: detail) { _ in }
        .frame(width: ViewerLayout.detailsWidth)
        .sampleSurface()
    }
  }

  #Preview("Cleaned up") {
    if let detail = PreviewStates.viewerSessionCleanedUp.details?.hostSession {
      SessionDetailGroups(detail: detail) { _ in }
        .frame(width: ViewerLayout.detailsWidth)
        .sampleSurface()
    }
  }

  #Preview("Interactive") {
    @Previewable @State var last = String("Nothing pressed")
    if let detail = PreviewStates.viewerSessionDetail.details?.hostSession {
      InteractivePreview {
        Text(last)
      } content: {
        SessionDetailGroups(detail: detail) { event in last = String(describing: event) }
          .frame(width: ViewerLayout.detailsWidth)
      }
    }
  }
#endif
