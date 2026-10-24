#if DEBUG
  import SwiftUI

  /// The brand marks' samples at the sizes v3's brand sheet checks (B1): the app icon by its tile, and the account
  /// picture in its circle.
  struct BrandSamples: View {
    /// Which mark the samples show.
    enum Kind: Sendable {
      /// The app icon at each tile v3 names: Welcome 88, About 84, Done 80, alerts 52, pairing 48, then 32 and 16.
      case appIcon
      /// The account picture at 64, the Helper pane's 40 and Choose's 30.
      case accountPicture
    }

    /// The app icon's tiles, largest first.
    static let tiles: [CGFloat] = [88, 84, 80, 52, 48, 32, 16]
    /// The account picture's sides, largest first.
    static let pictures: [CGFloat] = [64, 40, 30]

    private let kind: Kind

    /// Creates the samples of one mark.
    ///
    /// - Parameter kind: Which mark the samples show.
    init(_ kind: Kind) {
      self.kind = kind
    }

    /// That mark's samples, on the ground.
    var body: some View {
      HStack(alignment: .bottom, spacing: Spacing.l) {
        switch kind {
        case .appIcon:
          ForEach(Self.tiles, id: \.self) { tile in
            AppIcon(tile: tile)
              .frame(width: tile, height: tile)
          }
        case .accountPicture:
          ForEach(Self.pictures, id: \.self) { side in
            AccountPicture(side: side)
          }
        }
      }
      .sampleSurface()
    }
  }
#endif
