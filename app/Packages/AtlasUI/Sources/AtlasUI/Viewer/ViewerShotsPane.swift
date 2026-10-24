import SwiftUI

/// The Screenshots tab: the screenshots agents took, newest first, four to a row under Today, the parts of earlier
/// today and the days before, with a search by time; or why none shows.
struct ViewerShotsPane: View {
  /// What the pane shows.
  let state: ViewerShotsState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  var body: some View {
    VStack(spacing: .zero) {
      PaneHeader(title: "Screenshots the agent took", meta: state.meta) {
        SearchField(text: state.query, prompt: "Search by time") { send(.searchShots($0)) }
      }
      content
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    .clipShape(RoundedRectangle(cornerRadius: Radius.large))
    .cardSurface()
    .accessibilityElement(children: .contain)
    .accessibilityLabel("Screenshots the agent took")
  }

  /// The groups of tiles, or why there are none.
  @ViewBuilder private var content: some View {
    if let empty = state.empty {
      EmptyMessage(state: empty, ground: .pane)
        .padding(Spacing.xxl)
    } else {
      ScrollView {
        VStack(alignment: .leading, spacing: Spacing.xl) {
          ForEach(state.days) { day in
            GroupLabel(text: day.title)
            LazyVGrid(columns: columns, alignment: .leading, spacing: ViewerLayout.shotRowGap) {
              ForEach(day.shots) { tile in
                ShotTileView(tile: tile, send: send)
              }
            }
          }
        }
        .padding(.vertical, Spacing.xxl)
        .padding(.horizontal, ViewerLayout.paneTextInset)
      }
      .scrollIndicators(.automatic)
    }
  }

  /// Four columns sharing the width, 16 apart.
  private var columns: [GridItem] {
    Array(repeating: GridItem(.flexible(), spacing: Spacing.xxl, alignment: .top), count: ViewerLayout.shotColumns)
  }
}

#if DEBUG
  #Preview("Screenshots") {
    if case .screenshots(let shots) = PreviewStates.viewerShots.pane {
      ViewerShotsPane(state: shots) { _ in }
        .frame(width: ViewerLayout.largeShotLargest.width, height: ViewerLayout.largeShotLargest.height)
        .sampleSurface()
    }
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerShots
    InteractivePreview {
      if case .screenshots(let shots) = state.pane {
        ViewerShotsPane(state: shots) { event in
          state = PreviewStates.viewer(state, applying: event)
        }
        .frame(width: ViewerLayout.largeShotLargest.width, height: ViewerLayout.largeShotLargest.height)
      }
    }
  }
#endif
