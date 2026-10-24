import SwiftUI

/// Atlas's identity, the existing update controls, and the setup destination in separate groups.
struct AboutPane: View {
  /// What the pane shows.
  let state: SettingsAboutState
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.sectionGap) {
      identity
      GroupedSection([GeneralRow.picture], title: "Updates", description: "Check for a newer Atlas release") { _ in
        VStack(spacing: Spacing.m) {
          AboutUpdateButton(update: state.update, send: send)
          AboutUpdateStatus(status: state.status, send: send)
        }
        .multilineTextAlignment(.center)
        .frame(maxWidth: .infinity)
        .padding(Spacing.m)
      }
      GroupedSection([GeneralRow.openAtLogin], title: "Setup", description: "Open Atlas's setup window.") { _ in
        ListRow("Getting Started…", trailing: .button("Getting Started…"), size: .compact) { send(.gettingStarted) }
      }
    }
  }

  /// Atlas's existing identity, version and product explanation.
  private var identity: some View {
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
