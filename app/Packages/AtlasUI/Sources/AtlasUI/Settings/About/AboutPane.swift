import SwiftUI

/// The About pane (v3 G9): Atlas's icon, name, version and what it is, then the update button beside Getting
/// Started…, and what it says about updates in one spot under them.
struct AboutPane: View {
  /// What the pane shows.
  let state: SettingsAboutState
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    VStack(spacing: .zero) {
      // Laid out by its visible tile, as v3 spaces it; the icon's transparent margin overhangs.
      AppIcon(tile: SettingsLayout.aboutTile)
        .frame(width: SettingsLayout.aboutTile, height: SettingsLayout.aboutTile)
      Text(verbatim: "Atlas")
        .textStyle(.title2, weight: .bold)
        .accessibilityAddTraits(.isHeader)
        .padding(.top, Spacing.mPlus)
      Text(state.version)
        .font(Typography.digits(.callout))
        .foregroundStyle(Color.textSecondary)
        .padding(.top, Spacing.half)
      Text(verbatim: "A hidden second desktop where agents test apps while you keep working.")
        .textStyle(.body)
        .foregroundStyle(Color.textSecondary)
        .frame(maxWidth: SettingsLayout.aboutTextWidth)
        .fixedSize(horizontal: false, vertical: true)
        .padding(.top, Spacing.sPlus)
      HStack(spacing: Spacing.s) {
        AboutUpdateButton(update: state.update, send: send)
        Button("Getting Started…") { send(.gettingStarted) }
          .buttonStyle(.capsule(.glass, size: .mini))
      }
      .padding(.top, Spacing.xl)
      AboutUpdateStatus(status: state.status)
        .padding(.top, Spacing.m)
    }
    .multilineTextAlignment(.center)
    .frame(maxWidth: .infinity)
  }
}

#if DEBUG
  #Preview("About") {
    FormColumn(insets: SettingsLayout.aboutInsets) {
      AboutPane(state: PreviewStates.settingsAbout.about) { _ in }
    }
    .frame(width: SettingsLayout.width)
    .sampleSurface()
  }
#endif
