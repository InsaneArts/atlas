import SwiftUI

/// The About pane: Atlas's icon, name, version and what it is, then native update and Getting Started… buttons,
/// and what it says about updates in one spot under them, which keeps its height whatever it says.
struct AboutPane: View {
  /// What the pane shows.
  let state: SettingsAboutState
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    VStack(spacing: .zero) {
      // Laid out by its visible tile; the icon's transparent margin overhangs.
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
      Text(verbatim: "A hidden second desktop where agents test your apps while you work.")
        .textStyle(.body)
        .foregroundStyle(Color.textSecondary)
        .frame(maxWidth: SettingsLayout.aboutTextWidth)
        .fixedSize(horizontal: false, vertical: true)
        .padding(.top, Spacing.sPlus)
      HStack(spacing: Spacing.m) {
        AboutUpdateButton(update: state.update, send: send)
        Button("Getting Started…") { send(.gettingStarted) }
          .buttonStyle(.bordered)
      }
      .controlSize(.regular)
      .padding(.top, Spacing.xl)
      AboutUpdateStatus(status: state.status, send: send)
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

  #Preview("About — Update available") {
    FormColumn(insets: SettingsLayout.aboutInsets) {
      AboutPane(state: PreviewStates.settingsAboutNewer.about) { _ in }
    }
    .frame(width: SettingsLayout.width)
    .sampleSurface()
  }
#endif
