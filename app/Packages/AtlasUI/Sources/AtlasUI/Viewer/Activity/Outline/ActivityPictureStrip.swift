import SwiftUI

/// A folded run's or an ended session's newest pictures under its row, at its text's edge, then "120 pictures ›" while
/// it holds more, which opens them in place. Beside the link only as many as fit on one line show, so the link never
/// wraps; with no link every picture shows and the strip may wrap.
struct ActivityPictureStrip: View {
  /// The strip.
  let strip: ViewerPictureStrip
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.outlineWidth) private var width

  /// The pictures and the link.
  var body: some View {
    Group {
      if strip.hasLink {
        HStack(spacing: ViewerLayout.pictureGap) {
          tiles(Array(strip.tiles.prefix(ViewerLayout.stripCount(width: width))))
          Button(strip.link) {
            send(strip.opensSession ? .toggleSession(strip.run, opens: true) : .showRunPictures(strip.run))
          }
          .buttonStyle(OutlineLinkStyle())
          .padding(.leading, ViewerLayout.stripLinkLead)
          .help(strip.opensSession ? "Show this session's runs" : "Show every picture of this run")
        }
      } else {
        PictureFlow(spacing: ViewerLayout.pictureGap) {
          tiles(strip.tiles)
        }
      }
    }
    .padding(.bottom, ViewerLayout.stripBelow)
  }

  /// The pictures, each opening in the picture sheet.
  ///
  /// - Parameter tiles: The pictures shown.
  /// - Returns: The pictures.
  private func tiles(_ tiles: [ViewerPictureTile]) -> some View {
    ForEach(tiles) { tile in
      ActivityPictureButton(
        tile: tile,
        size: ViewerLayout.pictureFolded,
        inset: ViewerLayout.pictureFoldedInset,
        send: send
      )
    }
  }
}
