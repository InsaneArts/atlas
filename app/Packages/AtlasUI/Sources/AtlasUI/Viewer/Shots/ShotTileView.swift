import SwiftUI

/// One screenshot in the grid: its thumbnail at the screen's shape, a brighter ring under the pointer with nothing
/// moving, as a Finder icon highlights rather than jumps, ringed in blue while selected, and its time under it. A click selects it, a double click shows it large, zooming
/// from here, and it can be dragged out where the app allows. While it is shown large its picture has left the grid
/// for the large view, so its place stays empty.
struct ShotTileView: View {
  /// The screenshot.
  let tile: ViewerShotTile
  /// Whether it is the one shown large.
  var isOpen = false
  /// The namespace its picture zooms in, the window's.
  let zoom: Namespace.ID
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.shotDrag) private var drag
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  @Environment(\.paneShows) private var paneShows
  @State private var isHovered = false

  /// The tile, read as one button that opens it.
  var body: some View {
    draggable
      .onAppear { send(.thumbnailNeeded(tile.id)) }
      // A kept pane's tiles do not appear anew as it shows again; a thumbnail let go meanwhile is asked for again.
      .onChange(of: paneShows) { _, shows in if shows { send(.thumbnailNeeded(tile.id)) } }
      // How to use a tile lives on the tile, so the pane's header says only how many there are.
      .help(String("Space to open · Drag to use"))
      .accessibilityElement(children: .combine)
      .accessibilityLabel(tile.label)
      .accessibilityAddTraits(tile.isSelected ? [.isButton, .isSelected] : .isButton)
      .accessibilityAction { send(.openShot(tile.id)) }
  }

  /// The tile, handed to the app's drag where it has one.
  @ViewBuilder private var draggable: some View {
    if let drag {
      shown
        .onDrag { drag.item(tile.id) }
    } else {
      shown
    }
  }

  /// The thumbnail over its time, taking clicks.
  private var shown: some View {
    VStack(alignment: .leading, spacing: Spacing.xsPlus) {
      picture
      Text(tile.time)
        .font(Typography.digits(.caption))
        .foregroundStyle(Color.textTertiary)
        .lineLimit(1)
        .padding(.horizontal, Spacing.half)
    }
    .contentShape(Rectangle())
    .onTapGesture { send(.selectShot(tile.id)) }
    .simultaneousGesture(
      TapGesture(count: 2)
        .onEnded { send(.openShot(tile.id)) }
    )
    .onHover { isHovered = $0 }
    // Its picture leaves for the large view and comes back on the zoom's spring, as the large view's does.
    .animation(Motion.animation(Motion.zoom, reduceMotion: reduceMotion, reduced: Motion.reduced), value: isOpen)
  }

  /// The thumbnail, ringed while the pointer is on it; its place, empty, while it is shown large.
  @ViewBuilder private var picture: some View {
    if isOpen {
      Color.clear
        .aspectRatio(ViewerLayout.placeholderAspect, contentMode: .fit)
    } else {
      PictureTile(picture: tile.thumbnail, isSelected: tile.isSelected, isHighlighted: isHovered)
        .aspectRatio(ViewerLayout.placeholderAspect, contentMode: .fit)
        .shotZoom(id: tile.id, in: zoom)
        .elevation(.resting)
        .animation(Motion.hover(isHovered), value: isHovered)
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    @Previewable @Namespace var zoom
    if case .screenshots(let shots) = PreviewStates.viewerShots.pane {
      HStack(spacing: Spacing.l) {
        ForEach(shots.days.flatMap(\.shots).prefix(2)) { tile in
          ShotTileView(tile: tile, zoom: zoom) { _ in }
            .frame(width: SampleWidth.narrow)
        }
      }
      .padding(Spacing.s)
      .sampleSurface()
    }
  }
#endif
