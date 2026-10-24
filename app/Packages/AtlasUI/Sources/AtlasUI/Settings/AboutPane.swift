import SwiftUI

/// The About pane: Atlas's icon, name, version and what it is, then Check for Updates and Getting Started….
struct AboutPane: View {
  /// What the pane shows.
  let state: SettingsAboutState
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  var body: some View {
    VStack(spacing: Spacing.l) {
      AppIcon(size: SettingsLayout.aboutIcon)
      Text("Atlas")
        .font(Typography.font(.title3, weight: .bold))
        .accessibilityAddTraits(.isHeader)
      Text(state.version)
        .font(Typography.font(.body))
        .foregroundStyle(Color.textTertiary)
      Text("A hidden second desktop where agents test apps while you keep working.")
        .font(Typography.font(.body))
        .foregroundStyle(Color.textSecondary)
        .frame(maxWidth: SettingsLayout.aboutTextWidth)
        .fixedSize(horizontal: false, vertical: true)
      HStack(spacing: Spacing.m) {
        updateButton
        Button("Getting Started…") { send(.gettingStarted) }
          .buttonStyle(.capsule(.glass, size: .small))
      }
      .padding(.top, Spacing.m)
      if let answer = state.answer {
        Text(answer)
          .font(Typography.font(.callout))
          .foregroundStyle(Color.textTertiary)
          .frame(maxWidth: SettingsLayout.aboutTextWidth)
          .fixedSize(horizontal: false, vertical: true)
      }
    }
    .multilineTextAlignment(.center)
    .frame(maxWidth: .infinity, maxHeight: .infinity)
  }

  /// Check for Updates, or Install Update… once a newer version is found, each waiting while it runs.
  @ViewBuilder private var updateButton: some View {
    switch state.update {
    case .check:
      Button("Check for Updates") { send(.checkForUpdates) }
        .buttonStyle(.capsule(.primary, size: .small))
    case .checking:
      Button("Checking…") {}
        .buttonStyle(.capsule(.primary, size: .small))
        .disabled(true)
    case .install:
      Button("Install Update…") { send(.askInstall) }
        .buttonStyle(.capsule(.primary, size: .small))
    case .installing:
      Button("Installing…") {}
        .buttonStyle(.capsule(.primary, size: .small))
        .disabled(true)
    }
  }
}

#if DEBUG
  #Preview("About") {
    AboutPane(state: PreviewStates.settingsAbout.about) { _ in }
      .frame(width: SettingsLayout.window.width, height: SettingsLayout.window.height)
      .sampleSurface()
  }
#endif
