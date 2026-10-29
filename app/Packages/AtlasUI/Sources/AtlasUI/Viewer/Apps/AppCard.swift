import SwiftUI

/// An app open in the helper as one card: its head, its front window's picture or where its windows sit, its tags,
/// then its windows front to back, the whole row of each Show on Screen, and "N more" for the rest, or "No windows".
/// It rests on a quiet tile; the app in front is raised in place, white with a soft shadow in Light, so cards never
/// reorder as the front app changes. No strokes anywhere.
struct AppCard: View {
  /// The card.
  let card: ViewerAppCard
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The card's parts on its tile.
  var body: some View {
    VStack(alignment: .leading, spacing: ViewerLayout.appCardParts) {
      AppCardHead(card: card)
      if let picture = card.picture {
        AppCardPicture(picture: picture)
      }
      tags
      if card.windows.isEmpty {
        Text(verbatim: "No windows")
          .font(Typography.font(.callout))
          .foregroundStyle(Color.textTertiary)
          .frame(height: ViewerLayout.appCardWindowHeight)
      } else {
        windows
      }
    }
    .padding(ViewerLayout.appCardPadding)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    .background(tile)
    .accessibilityElement(children: .contain)
  }

  /// The tile under the card: quiet at rest, raised in place while its app is in front. The two tiles fade into each
  /// other rather than one colour turning into the other, which would pass through a darker grey on the way, since
  /// the quiet tile is a faint tint and the raised one solid.
  private var tile: some View {
    let shape = RoundedRectangle(cornerRadius: ViewerLayout.appCardRadius, style: .continuous)
    let motion = AppsMotion.front(reduceMotion: reduceMotion)
    // Only the tiles' opacity takes the raise, so the tile moves with its card as the grid moves it.
    return ZStack {
      shape
        .fill(Color.fillTile)
        .animation(motion) { tile in
          tile.opacity(card.isFront ? 0 : 1)
        }
      shape
        .fill(Color.fillTileRaised)
        .shadow(color: Elevation.selection.color, radius: Elevation.selection.radius, y: Elevation.selection.y)
        .animation(motion) { tile in
          tile.opacity(card.isFront ? 1 : 0)
        }
    }
  }

  /// The tags, as many as the card's width holds, the last ones giving way first, in a row a tag high even while it
  /// holds none, so a card never grows or shrinks as "In front" comes and goes. Each way of fitting is kept by how
  /// many tags it leaves out, so the one holding every tag stays the same view as tags come and go, and a tag fades
  /// in or out in it while the others stand still.
  private var tags: some View {
    ViewThatFits(in: .horizontal) {
      ForEach(0..<max(card.tags.count, 1), id: \.self) { left in
        HStack(spacing: ViewerLayout.appCardTagGap) {
          ForEach(card.tags.dropLast(left)) { tag in
            Tag(text: tag.words, tone: tone(tag.tone))
              .transition(AppsMotion.tag(reduceMotion: reduceMotion))
          }
        }
      }
    }
    .frame(height: Tag.height, alignment: .leading)
  }

  /// The windows the card lists, then "N more".
  private var windows: some View {
    let (listed, more) = Self.listed(card.windows)
    return VStack(spacing: ViewerLayout.appCardWindowGap) {
      ForEach(listed) { window in
        AppWindowRow(window: window) {
          send(.showWindow(app: card.id, window: window.id))
        }
      }
      if !more.isEmpty {
        AppMoreWindows(app: card.id, windows: more, send: send)
      }
    }
  }

  /// The windows a card lists and those it puts in "N more": every one up to ``ViewerLayout/appCardWindowsShown``,
  /// else one fewer, so "N more" takes the last row and the card is never taller than that many rows.
  ///
  /// - Parameter windows: The app's windows, front to back.
  /// - Returns: The windows listed, and the rest.
  static func listed(_ windows: [ViewerWindowRow]) -> (listed: [ViewerWindowRow], more: [ViewerWindowRow]) {
    let most = ViewerLayout.appCardWindowsShown
    guard windows.count > most else { return (windows, []) }
    return (Array(windows.prefix(most - 1)), Array(windows.dropFirst(most - 1)))
  }

  /// A tag's tone as the tag draws it.
  ///
  /// - Parameter tone: The card's tone for it.
  /// - Returns: The tag's.
  private func tone(_ tone: ViewerAppTag.Tone) -> Tag.Tone {
    switch tone {
    case .quiet: .quiet
    case .accent: .accent
    case .attention: .attention
    }
  }
}

#if DEBUG
  #Preview("Cards") {
    AppsSamples(.cards)
  }
#endif
