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
  /// The picture the sheet showed last, or takes focus back to, which keeps its place in the strip after the sheet
  /// closes, so focus has it to come back to.
  @State private var pinned: String?

  /// The pictures and the link.
  var body: some View {
    Group {
      if strip.hasLink {
        HStack(spacing: ViewerLayout.pictureGap) {
          tiles(shown)
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
    .onChange(of: strip.tiles.first { $0.isCurrent || $0.returnsFocus }?.id, initial: true) { _, held in
      // Kept through the sheet's closing; the next picture the sheet shows here takes its place.
      if let held { pinned = held }
    }
  }

  /// The newest pictures that fit beside the link; the one the sheet showed last, or takes focus back to, stands last
  /// among them when it would not fit, so focus always has a picture to come back to.
  private var shown: [ViewerPictureTile] {
    var shown = Array(strip.tiles.prefix(ViewerLayout.stripCount(width: width)))
    let held = strip.tiles.first { $0.isCurrent || $0.returnsFocus } ?? strip.tiles.first { $0.id == pinned }
    if let held, !shown.contains(where: { $0.id == held.id }), !shown.isEmpty {
      shown[shown.count - 1] = held
    }
    return shown
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
