import SwiftUI

/// The Screenshots list, inside the card with no ground of its own: the screenshots agents took, newest first, in one
/// adaptive grid under Now, Earlier today and the days before, their headings scrolling with the tiles; or why none
/// shows. It scrolls in the card's one list scroll (``CardScroll``), under the small screen, and a step to a tile
/// scrolls it into view only as far as it must, clear of the small screen. The time groups find a picture, so the
/// search leaves it be. A screenshot shown large has zoomed out of its tile, so its place in the grid waits empty.
/// The grid tells the window how many tiles stand in a row, which ↑ and ↓ step by. While the window is being resized
/// it keeps its count and stretches its tiles within it, so the tiles never jump from row to row under the pointer;
/// let go, the count follows the width and the tiles move to their places on ``Motion/state``.
struct ViewerShotsPane: View {
  /// What the pane shows.
  let state: ViewerShotsState
  /// The namespace a screenshot zooms in between its tile and the large view, the window's.
  let zoom: Namespace.ID
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.paneShows) private var paneShows
  @Environment(\.motionReduced) private var reduceMotion

  /// The tiles, or why there are none.
  var body: some View {
    content
      .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
      .accessibilityElement(children: .contain)
      .modifier(ScreenshotFocus(enabled: paneShows && state.large == nil))
      .accessibilityLabel("Screenshots")
      .accessibilityValue(state.meta)
  }

  /// The groups of tiles, or why there are none.
  @ViewBuilder private var content: some View {
    if let empty = state.empty {
      EmptyMessage(state: empty, size: .stage, ground: .pane, symbol: .photos)
    } else {
      CardScroll(reveal: reveal) {
        grid
      }
    }
  }

  /// Every group's heading and tiles in one grid, each group starting a row of its own.
  private var grid: some View {
    let place = state.tileShape
    return LazyVGrid(columns: columns, alignment: .leading, spacing: ViewerLayout.shotGap) {
      ForEach(Array(state.days.enumerated()), id: \.element.id) { index, day in
        Section {
          ForEach(day.shots) { tile in
            ShotTileView(tile: tile, place: place, isOpen: tile.id == state.large?.id, zoom: zoom, send: send)
              .listSelection(tile.id, isSelected: tile.isSelected)
          }
        } header: {
          ListHeading(title: day.title)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.top, index == .zero ? ViewerLayout.shotHeadingTop : ViewerLayout.shotHeadingAbove)
        }
      }
    }
    .scrollTargetLayout()
    .animation(Motion.layout(Motion.state, reduceMotion: reduceMotion), value: state.heldColumns)
    .padding(.horizontal, ViewerLayout.swapInset)
    .padding(.bottom, ViewerLayout.swapInset)
    .onGeometryChange(for: Int.self) { proxy in
      Self.columns(across: proxy.size.width)
    } action: { columns in
      send(.shotColumns(state.heldColumns ?? columns))
    }
  }

  /// The selected tile, which each keyboard step scrolls into view.
  private var reveal: CardScrollReveal {
    CardScrollReveal(
      id: state.days
        .flatMap(\.shots)
        .first(where: \.isSelected)?
        .id,
      request: state.scrollRequest
    )
  }

  /// As many columns as fit, each at least ``ViewerLayout/shotLeastWidth`` wide, sharing the width; while the window
  /// is being resized, the count it held as the resize began, sharing whatever width there is.
  private var columns: [GridItem] {
    guard let held = state.heldColumns else {
      return [GridItem(.adaptive(minimum: ViewerLayout.shotLeastWidth), spacing: ViewerLayout.shotGap, alignment: .top)]
    }
    return Array(
      repeating: GridItem(.flexible(), spacing: ViewerLayout.shotGap, alignment: .top),
      count: max(held, 1)
    )
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
