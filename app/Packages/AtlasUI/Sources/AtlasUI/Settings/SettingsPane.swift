import SwiftUI

/// Stable page identities retain independent scrolling; only the selected page accepts input.
struct SettingsPane: View {
  /// What the window shows.
  let state: SettingsState
  /// The original preference writer retained by the window's public interface.
  let edit: @MainActor (SettingsGeneralState) -> Void
  /// Receives navigation, field edits and existing actions.
  let send: @MainActor (SettingsEvent) -> Void
  /// Only the immediately preceding page may still draw during a navigation crossfade.
  var outgoing: SettingsDestination? = nil

  /// Each route keeps its scroll view through navigation and shared status updates.
  var body: some View {
    ZStack(alignment: .top) {
      ForEach(SettingsDestination.pages, id: \.self) { destination in
        ScrollViewReader { scroll in
          FormColumn(insets: SettingsLayout.swapInsets) {
            VStack(alignment: .leading, spacing: SettingsLayout.sectionGap) {
              Text(destination.description)
                .textStyle(.callout)
                .foregroundStyle(Color.textSecondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .settingsTarget(.page(destination))
              page(destination)
            }
            .frame(maxWidth: SettingsLayout.contentMaximumWidth)
            .frame(maxWidth: .infinity)
          }
          .environment(
            \.settingsReveal,
            state.location == destination && !state.search.isActive ? state.reveal : nil
          )
          .onChange(of: state.reveal) { _, reveal in
            if let reveal, reveal.scrolls, reveal.target.destination == destination { scroll.scrollTo(reveal.target) }
          }
        }
        .modifier(
          SettingsVisibility(
            active: state.location == destination && !state.search.isActive,
            retained: (state.location == destination && !state.search.isActive) || outgoing == destination
          )
        )
      }
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    .environment(\.marksFade, true)
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
