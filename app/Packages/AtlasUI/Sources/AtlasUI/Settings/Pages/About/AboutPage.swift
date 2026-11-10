import SwiftUI

/// The About page: the app's icon, name, tagline and version, over a card with its software update and Getting
/// Started.
struct AboutPage: View {
  /// What the pane shows.
  let state: SettingsAboutState
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// The app's identity, then updates and the setup walkthrough in one card.
  var body: some View {
    VStack(spacing: Spacing.xl) {
      header
      GroupedSection(AboutRow.allCases) { row in
        switch row {
        case .updates:
          ListRowBody("Software update", size: .compact) {
            AboutUpdateStatus(status: state.status, send: send)
          } trailing: {
            AboutUpdateButton(update: state.update, send: send)
          }
        case .gettingStarted:
          ListRow(
            "Getting Started",
            detail: "Walk through setup again",
            trailing: .button("Open…"),
            size: .compact
          ) { send(.gettingStarted) }
        }
      }
    }
    .frame(maxWidth: .infinity)
  }

  /// The icon and the app's name, a wrapping tagline and a quiet version caption.
  private var header: some View {
    VStack(spacing: Spacing.xs) {
      // Laid out by its visible tile; the icon's transparent margin overhangs.
      AppIcon(tile: SettingsLayout.aboutTile)
        .frame(width: SettingsLayout.aboutTile, height: SettingsLayout.aboutTile)
        .padding(.bottom, Spacing.s)
      Text(verbatim: "Atlas")
        .textStyle(.subheading)
        .accessibilityAddTraits(.isHeader)
      Text(verbatim: "A hidden second desktop where agents test your apps while you work.")
        .textStyle(.body)
        .foregroundStyle(Color.textSecondary)
        .lineLimit(2)
        .frame(maxWidth: SettingsLayout.aboutTextWidth)
        .fixedSize(horizontal: false, vertical: true)
      Text(state.version)
        .textStyle(.caption)
        .foregroundStyle(Color.textSecondary)
        .padding(.top, Spacing.half)
    }
    .multilineTextAlignment(.center)
    .frame(maxWidth: .infinity)
    .padding(.top, Spacing.l)
  }
}

#if DEBUG
  #Preview("About") {
    SettingsPageSample { AboutPage(state: PreviewStates.settingsAbout.about) { _ in } }
  }

  #Preview("About — Every update state, narrow") {
    ScrollView {
      VStack(spacing: SettingsLayout.sectionGap) {
        ForEach(PreviewStates.settingsAboutSamples) { sample in
          VStack(alignment: .leading, spacing: SettingsLayout.titleGap) {
            SectionTitle(text: sample.name)
            AboutPage(state: sample.value.about) { _ in }
          }
          .padding(SettingsLayout.pageInsets)
        }
      }
    }
    .frame(width: PreviewStates.settingsNarrowWidth)
    .sampleSurface()
  }
#endif
