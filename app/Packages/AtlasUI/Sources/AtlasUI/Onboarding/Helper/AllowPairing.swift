import SwiftUI

/// The helper's allow pairing: the asking account's picture in its circle, as macOS shows it at login, or its initial
/// on a disc where it has none, a quiet chevron, then Atlas's tile, both 48 points. VoiceOver skips it, as the title
/// says who asks.
struct AllowPairing: View {
  /// The asking account's first letter, drawn where it has no picture.
  let initial: String
  /// The asking account's picture, or nil for none.
  var picture: IconImage?

  /// The account, the chevron and Atlas's tile, 14 apart.
  var body: some View {
    HStack(spacing: Spacing.mPlus) {
      account
      Image(systemName: SymbolName.next.rawValue)
        .font(Typography.symbol(.control))
        .foregroundStyle(Color.textTertiary)
      // Laid out by its tile, its canvas's margin reaching past the frame, so the gap is the tile's.
      AppIcon(tile: OnboardingLayout.pairingTile)
        .frame(width: OnboardingLayout.pairingTile, height: OnboardingLayout.pairingTile)
    }
    .accessibilityHidden(true)
  }

  /// The asking account: its picture filling a circle, never stretched, else its initial on a disc.
  @ViewBuilder private var account: some View {
    if let picture {
      Image(decorative: picture.image, scale: 1)
        .resizable()
        .interpolation(.high)
        .scaledToFill()
        .frame(width: OnboardingLayout.pairingTile, height: OnboardingLayout.pairingTile)
        .clipShape(Circle())
    } else {
      Disc(content: .letter(initial), tone: .avatar, size: .pairing)
    }
  }
}

#if DEBUG
  #Preview("Pairing") {
    AllowPairing(initial: String("I"))
      .padding(Spacing.xl)
      .sampleSurface()
  }
#endif
