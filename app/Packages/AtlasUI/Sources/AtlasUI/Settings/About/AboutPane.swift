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
        .settingsTarget(.identity)
      GroupedSection(
        [GeneralRow.picture],
        title: SettingsSearchCatalog.updates.section,
        description: SettingsSearchCatalog.updates.sectionDetail
      ) { _ in
        VStack(spacing: Spacing.m) {
          AboutUpdateButton(update: state.update, send: send)
          AboutUpdateStatus(status: state.status, send: send)
        }
        .multilineTextAlignment(.center)
        .frame(maxWidth: .infinity)
        .padding(Spacing.m)
        .settingsTarget(.updates)
      }
      GroupedSection(
        [GeneralRow.openAtLogin],
        title: SettingsSearchCatalog.setup.section,
        description: SettingsSearchCatalog.setup.sectionDetail
      ) { _ in
        ListRow(SettingsSearchCatalog.setup.title, trailing: .button("Getting Started…"), size: .compact) {
          send(.gettingStarted)
        }
        .settingsTarget(.setup)
      }
    }
  }

  /// Atlas's existing identity, version and product explanation.
  private var identity: some View {
    VStack(spacing: .zero) {
      // Laid out by its visible tile; the icon's transparent margin overhangs.
      AppIcon(tile: SettingsLayout.aboutTile)
        .frame(width: SettingsLayout.aboutTile, height: SettingsLayout.aboutTile)
      Text(verbatim: SettingsSearchCatalog.identity.title)
        .textStyle(.title2, weight: .bold)
        .accessibilityAddTraits(.isHeader)
        .padding(.top, Spacing.mPlus)
      Text(state.version)
        .font(Typography.digits(.callout))
        .foregroundStyle(Color.textSecondary)
        .padding(.top, Spacing.half)
      Text(verbatim: SettingsSearchCatalog.identity.detail)
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
