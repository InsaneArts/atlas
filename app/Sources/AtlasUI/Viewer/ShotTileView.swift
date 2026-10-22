import SwiftUI

/// One screenshot in the grid: its thumbnail at the screen's shape, ringed while selected, and its time under it.
/// A click selects it, a double click shows it large, and it can be dragged out where the app allows.
struct ShotTileView: View {
  /// The screenshot.
  let tile: ViewerShotTile
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.shotDrag) private var drag

  var body: some View {
    draggable
      .onAppear { send(.thumbnailNeeded(tile.id)) }
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
    VStack(alignment: .leading, spacing: Spacing.m) {
      PictureTile(picture: tile.thumbnail, isSelected: tile.isSelected)
        .aspectRatio(ViewerLayout.placeholderAspect, contentMode: .fit)
      Text(tile.time)
        .font(Typography.font(.callout))
        .foregroundStyle(Color.textTertiary)
        .monospacedDigit()
        .lineLimit(1)
        .padding(.horizontal, Spacing.xxs)
    }
    .contentShape(Rectangle())
    .onTapGesture { send(.selectShot(tile.id)) }
    .simultaneousGesture(
      TapGesture(count: 2)
        .onEnded { send(.openShot(tile.id)) }
    )
  }
}

#if DEBUG
  #Preview("Samples") {
    if case .screenshots(let shots) = PreviewStates.viewerShots.pane {
      HStack(spacing: Spacing.xxl) {
        ForEach(shots.days.flatMap(\.shots).prefix(2)) { tile in
          ShotTileView(tile: tile) { _ in }
            .frame(width: SampleWidth.narrow)
        }
      }
      .padding(Spacing.m)
      .sampleSurface()
    }
  }
#endif
