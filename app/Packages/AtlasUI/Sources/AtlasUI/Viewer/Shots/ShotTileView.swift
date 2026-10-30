import SwiftUI

/// One screenshot in the grid: the whole screenshot at its own shape, never cut, standing at the foot of a place the
/// grid's shape for every tile, so the rows stand even and every time lines up under its picture; a brighter edge
/// under the pointer with nothing moving, as a Finder icon highlights rather than jumps; a blue ring outside it while
/// selected, which comes and goes at once; and its time under it. A click selects it, a double click shows it large,
/// zooming from here, and it can be dragged out where the app allows. While it is shown large its picture has left
/// the grid for the large view, so its place stays empty.
struct ShotTileView: View {
  /// The screenshot.
  let tile: ViewerShotTile
  /// The shape of its place in the grid, every tile's (``ViewerShotsState/tileShape``).
  var place = ShotShape.standard
  /// Whether it is the one shown large.
  var isOpen = false
  /// The namespace its picture zooms in, the window's.
  let zoom: Namespace.ID
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.shotDrag) private var drag
  @Environment(\.motionReduced) private var reduceMotion

  @State private var isHovered = false

  /// The tile, read as one button that opens it.
  var body: some View {
    draggable
      .asksForThumbnail(tile.id, send: send)
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
    VStack(alignment: .leading, spacing: ViewerLayout.shotTimeGap) {
      picture
        .aspectRatio(place, contentMode: .fit)
      Text(tile.time)
        .font(Typography.digits(.caption))
        .foregroundStyle(Color.textTertiary)
        .lineLimit(1)
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

  /// The thumbnail at the screenshot's own shape, standing at the foot of its place, its edge brighter while the
  /// pointer is on it; its place, empty, while it is shown large.
  @ViewBuilder private var picture: some View {
    if isOpen {
      Color.clear
    } else {
      ShotThumbnail(
        picture: tile.thumbnail,
        shape: tile.shape,
        isSelected: tile.isSelected,
        isHighlighted: isHovered,
        zoom: (tile.id, zoom)
      )
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
