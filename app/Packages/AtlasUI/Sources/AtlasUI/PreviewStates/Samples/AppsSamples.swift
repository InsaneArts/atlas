#if DEBUG
  import SwiftUI

  /// The Apps list's pieces on their own: the sample's cards in a grid, and a card's window rows.
  struct AppsSamples: View {
    /// Which samples show.
    enum Kind: Sendable {
      /// The sample's cards in a grid three across.
      case cards
      /// The front app's window rows, the front one marked, and "N more".
      case windows
    }

    /// Which samples show.
    private let kind: Kind

    /// Creates the samples.
    ///
    /// - Parameter kind: Which samples show.
    init(_ kind: Kind) {
      self.kind = kind
    }

    /// The samples.
    var body: some View {
      switch kind {
      case .cards:
        VStack(alignment: .leading, spacing: ViewerLayout.appCardGap) {
          ForEach(PreviewStates.sampleAppsPane.groups) { group in
            AppCardGrid {
              ForEach(group.cards) { card in
                AppCard(card: card) { _ in }
              }
            }
          }
        }
        .frame(width: SampleWidth.narrow * 3)
        .sampleSurface()
      case .windows:
        AppCard(card: manyWindows) { _ in }
          .frame(width: ViewerLayout.appCardLeast)
          .sampleSurface()
      }
    }

    /// Opie with five windows, so its card lists two and "3 more windows".
    private var manyWindows: ViewerAppCard {
      var card = PreviewStates.sampleOpie(origin: PreviewStates.claudeOrigin)
      card.windows += [
        PreviewStates.window(13, "Appearance", "520 × 860"),
        PreviewStates.window(14, "Shortcuts", "520 × 860"),
        PreviewStates.window(15, "About Opie", "420 × 300"),
      ]
      return card
    }
  }
#endif
