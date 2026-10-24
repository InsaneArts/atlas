import SwiftUI

/// Stable page identities retain independent scrolling; only the selected page accepts input.
struct SettingsPane: View {
  /// What the window shows.
  let state: SettingsState
  /// The original preference writer retained by the window's public interface.
  let edit: @MainActor (SettingsGeneralState) -> Void
  /// Receives navigation, field edits and existing actions.
  let send: @MainActor (SettingsEvent) -> Void

  /// Each route keeps its scroll view through navigation and shared status updates.
  var body: some View {
    ZStack(alignment: .top) {
      ForEach(SettingsDestination.pages, id: \.self) { destination in
        FormColumn(insets: SettingsLayout.swapInsets) {
          VStack(alignment: .leading, spacing: SettingsLayout.sectionGap) {
            Text(destination.description)
              .textStyle(.callout)
              .foregroundStyle(Color.textSecondary)
              .frame(maxWidth: .infinity, alignment: .leading)
            page(destination)
          }
          .frame(maxWidth: SettingsLayout.contentMaximumWidth)
          .frame(maxWidth: .infinity)
        }
        .opacity(state.location == destination ? 1 : 0)
        .disabled(state.location != destination)
        .allowsHitTesting(state.location == destination)
        .accessibilityHidden(state.location != destination)
        .animation(state.location == destination ? Motion.swapIn : Motion.swapOut, value: state.location)
      }
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
  }

  /// Draws canonical controls once, with disclosure links from their owning roots.
  ///
  /// - Parameter destination: The page's stable identity.
  /// - Returns: Its sections.
  @ViewBuilder private func page(_ destination: SettingsDestination) -> some View {
    switch destination {
    case .root(.general): GeneralPane(state: state.general, send: send)
    case .root(.screen): ScreenPane(state: state.general, send: send)
    case .root(.notifications): NotificationsPane(state: state.general, send: send)
    case .root(.helper): HelperPane(state: state.helper, send: send)
    case .diagnostics: HelperDiagnosticsPane(state: state.helper, send: send)
    case .root(.agents): AgentsPane(state: state.agents, send: send)
    case .projects: ProjectsPane(state: state.agents, send: send)
    case .root(.about): AboutPane(state: state.about, send: send)
    }
  }
}
