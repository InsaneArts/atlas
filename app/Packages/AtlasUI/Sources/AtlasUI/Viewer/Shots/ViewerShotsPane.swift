import SwiftUI

/// The Screenshots tab: the screenshots agents took, newest first, in an adaptive grid under Now, Earlier today and the
/// days before, their headers pinned as the grid scrolls; or why none shows, on the pane's own ground. The time groups
/// find a picture, so there is no search. A screenshot shown large has zoomed out of its tile, so its place in the grid
/// waits empty. The grid tells the window how many tiles stand in a row, which ↑ and ↓ step by.
struct ViewerShotsPane: View {
  /// What the pane shows.
  let state: ViewerShotsState
  /// The namespace a screenshot zooms in between its tile and the large view, the window's.
  let zoom: Namespace.ID
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.paneSettled) private var paneSettled

  /// The header over the tiles, or why there are none.
  var body: some View {
    VStack(spacing: .zero) {
      PaneHeader(title: "Screenshots", meta: state.meta)
      content
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    .clipShape(RoundedRectangle(cornerRadius: Radius.large))
    .cardSurface()
    .accessibilityElement(children: .contain)
    .accessibilityLabel("Screenshots")
  }

  /// The groups of tiles, or why there are none.
  @ViewBuilder private var content: some View {
    if let empty = state.empty {
      EmptyMessage(state: empty, size: .stage, ground: .pane, symbol: .photos)
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
        .onGeometryChange(for: Int.self) { proxy in
          Self.columns(across: proxy.size.width)
        } action: { columns in
          send(.shotColumns(columns))
        }
      }
      .scrollIndicators(paneSettled ? .automatic : .hidden)
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
    .padding(.horizontal, ViewerLayout.swapInset)
    .padding(.bottom, ViewerLayout.swapInset)
  }

  /// As many columns as fit, each at least ``ViewerLayout/shotLeastWidth`` wide, sharing the width.
  private var columns: [GridItem] {
    [GridItem(.adaptive(minimum: ViewerLayout.shotLeastWidth), spacing: ViewerLayout.shotGap, alignment: .top)]
  }

  /// How many tiles stand in a row of a pane `width` wide, as the adaptive grid lays them out: as many of the least
  /// width, with their gaps, as fit inside the pane's insets, and at least one.
  ///
  /// - Parameter width: The pane's width.
  /// - Returns: The columns.
  nonisolated static func columns(across width: CGFloat) -> Int {
    let room = width - ViewerLayout.swapInset * 2 + ViewerLayout.shotGap
    return max(1, Int((room / (ViewerLayout.shotLeastWidth + ViewerLayout.shotGap)).rounded(.down)))
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
