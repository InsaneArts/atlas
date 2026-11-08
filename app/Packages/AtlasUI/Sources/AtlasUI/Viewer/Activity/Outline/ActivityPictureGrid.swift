import SwiftUI

/// A run's pictures opened in place, at its text's edge: fixed 128-wide columns 8 apart, at most six to a row, then
/// "Show N more ›" while it holds more, and Show fewer.
struct ActivityPictureGrid: View {
  /// The grid.
  let grid: ViewerPictureGrid
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.motionReduced) private var reduceMotion

  /// The grid and its links.
  var body: some View {
    VStack(alignment: .leading, spacing: ViewerLayout.gridLinksAbove) {
      LazyVGrid(columns: columns, alignment: .leading, spacing: ViewerLayout.pictureGap) {
        ForEach(grid.tiles) { tile in
          ActivityPictureButton(
            tile: tile,
            size: ViewerLayout.pictureGrid,
            send: send
          )
        }
      }
      .frame(maxWidth: ViewerLayout.pictureGridWidth, alignment: .leading)
      HStack(spacing: ViewerLayout.pictureGap) {
        if grid.more > 0 {
          Button(grid.moreTitle) {
            withAnimation(ActivityMotion.filter(reduceMotion: reduceMotion)) { send(.showAllRunPictures(grid.run)) }
          }
          .buttonStyle(OutlineLinkStyle())
        }
        Button("Show fewer") {
          withAnimation(ActivityMotion.filter(reduceMotion: reduceMotion)) { send(.hideRunPictures(grid.run)) }
        }
        .buttonStyle(OutlineLinkStyle(chevron: false))
      }
      // The links' words stand on the grid's edge, their capsules reaching past it.
      .padding(.leading, -ViewerLayout.linkInset)
    }
    .padding(.bottom, ViewerLayout.gridBelow)
  }

  /// As many fixed columns as fit, at most ``ViewerLayout/pictureGridColumns``.
  private var columns: [GridItem] {
    [
      GridItem(
        .adaptive(minimum: ViewerLayout.pictureGrid.width, maximum: ViewerLayout.pictureGrid.width),
        spacing: ViewerLayout.pictureGap,
        alignment: .leading
      )
    ]
  }
}
