import SwiftUI

/// A compact app header over the same grouped action rows as the other Settings panes.
struct AboutPane: View {
  /// What the pane shows.
  let state: SettingsAboutState
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// The app's identity, then updates and the setup walkthrough in one card.
  var body: some View {
    VStack(spacing: Spacing.l) {
      header
      GroupedSection(AboutRow.allCases) { row in
        switch row {
        case .updates:
          ListRowBody("Updates", size: .compact) {
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

  /// The icon and pane title, a wrapping tagline and a quiet version caption.
  private var header: some View {
    VStack(spacing: Spacing.s) {
      // Laid out by its visible tile; the icon's transparent margin overhangs.
      AppIcon(tile: SettingsLayout.aboutTile)
        .frame(width: SettingsLayout.aboutTile, height: SettingsLayout.aboutTile)
        .padding(.bottom, Spacing.xs)
      Text(verbatim: "Atlas")
        .textStyle(.heading)
        .accessibilityAddTraits(.isHeader)
      Text(verbatim: "A hidden second desktop where agents test your apps while you work.")
        .textStyle(.body)
        .foregroundStyle(Color.textSecondary)
        .lineLimit(2)
        .frame(maxWidth: SettingsLayout.aboutTextWidth)
        .fixedSize(horizontal: false, vertical: true)
      Text(state.version)
        .textStyle(.caption)
        .foregroundStyle(Color.textTertiary)
    }
    .multilineTextAlignment(.center)
    .frame(maxWidth: .infinity)
    .padding(.top, Spacing.l)
    .padding(.bottom, Spacing.xs)
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

  #Preview("About — Every update state, narrow") {
    ScrollView {
      VStack(spacing: SettingsLayout.sectionGap) {
        ForEach(PreviewStates.settingsAboutSamples) { sample in
          VStack(alignment: .leading, spacing: SettingsLayout.titleGap) {
            GroupLabel(text: sample.name)
            AboutPane(state: sample.value.about) { _ in }
          }
          .padding(SettingsLayout.aboutInsets)
        }
      }
    }
    .frame(width: PreviewStates.settingsNarrowWidth)
    .sampleSurface()
  }
#endif
