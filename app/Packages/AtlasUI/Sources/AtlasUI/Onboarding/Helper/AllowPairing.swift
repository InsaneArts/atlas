import SwiftUI

/// The helper's allow pairing (v3 `.pairing`): the asking account's initial on its avatar, a quiet chevron, then Atlas's
/// tile, both 48 points. VoiceOver skips it, as the title says who asks.
struct AllowPairing: View {
  /// The asking account's first letter.
  let initial: String

  /// The avatar, the chevron and Atlas's tile, 14 apart.
  var body: some View {
    HStack(spacing: Spacing.mPlus) {
      Disc(content: .letter(initial), tone: .avatar, size: .pairing)
      Image(systemName: SymbolName.next.rawValue)
        .font(Typography.symbol(.control))
        .foregroundStyle(Color.textTertiary)
      // Laid out by its tile, its canvas's margin reaching past the frame, so the gap is the tile's (v3).
      AppIcon(tile: OnboardingLayout.pairingTile)
        .frame(width: OnboardingLayout.pairingTile, height: OnboardingLayout.pairingTile)
    }
    .accessibilityHidden(true)
  }
}

#if DEBUG
  #Preview("Pairing") {
    AllowPairing(initial: String("I"))
      .padding(Spacing.xl)
      .sampleSurface()
  }
#endif
