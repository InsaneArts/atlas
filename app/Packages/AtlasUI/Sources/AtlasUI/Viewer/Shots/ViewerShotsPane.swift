import SwiftUI

/// The Screenshots tab (v3 V10): the screenshots agents took, newest first, in an adaptive grid under Today, the
/// parts of earlier today and the days before, their headers pinned as the grid scrolls, with a search by time; or
/// why none shows. A screenshot shown large has zoomed out of its tile, so its place in the grid waits empty.
struct ViewerShotsPane: View {
  /// What the pane shows.
  let state: ViewerShotsState
  /// The namespace a screenshot zooms in between its tile and the large view, the window's.
  let zoom: Namespace.ID
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The header with the search, over the tiles or why there are none.
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
      EmptyMessage(state: empty, size: .stage, symbol: .photos)
    } else {
      ScrollView {
        LazyVStack(alignment: .leading, spacing: .zero, pinnedViews: .sectionHeaders) {
          ForEach(state.days) { day in
            Section {
              grid(day)
            } header: {
              PinnedHeader(title: day.title)
            }
          }
        }
      }
      .scrollIndicators(.automatic)
    }
  }

  /// A group's tiles, as many to a row as fit.
  ///
  /// - Parameter day: The group.
  /// - Returns: The grid.
  private func grid(_ day: ViewerShotDay) -> some View {
    LazyVGrid(columns: columns, alignment: .leading, spacing: ViewerLayout.shotGap) {
      ForEach(day.shots) { tile in
        ShotTileView(tile: tile, isOpen: tile.id == state.large?.id, zoom: zoom, send: send)
      }
    }
    .padding(.horizontal, ViewerLayout.paneInset)
    .padding(.bottom, ViewerLayout.paneInset)
  }

  /// As many columns as fit, each at least ``ViewerLayout/shotLeastWidth`` wide, sharing the width.
  private var columns: [GridItem] {
    [GridItem(.adaptive(minimum: ViewerLayout.shotLeastWidth), spacing: ViewerLayout.shotGap, alignment: .top)]
  }
}

#if DEBUG
  #Preview("Screenshots") {
    @Previewable @Namespace var zoom
    if case .screenshots(let shots) = PreviewStates.viewerShots.pane {
      ViewerShotsPane(state: shots, zoom: zoom) { _ in }
        .frame(width: ViewerLayout.largeShotLargest.width, height: ViewerLayout.largeShotLargest.height)
        .sampleSurface()
    }
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerShots
    @Previewable @Namespace var zoom
    InteractivePreview {
      if case .screenshots(let shots) = state.pane {
        ViewerShotsPane(state: shots, zoom: zoom) { event in
          state = PreviewStates.viewer(state, applying: event)
        }
        .frame(width: ViewerLayout.largeShotLargest.width, height: ViewerLayout.largeShotLargest.height)
      }
    }
  }
#endif
