import SwiftUI

/// The existing panes in a stable scrolling viewport. Each pane keeps its scroll position during this session.
/// Only the selected pane accepts input or appears in the accessibility tree.
struct SettingsPane: View {
  /// What the window shows.
  let state: SettingsState
  /// Receives the General pane as the person changed it.
  let edit: @MainActor (SettingsGeneralState) -> Void
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    ZStack(alignment: .top) {
      ForEach(SettingsTab.allCases, id: \.self) { tab in
        pane(tab)
          .opacity(state.tabs.selected == tab ? 1 : 0)
          .disabled(state.tabs.selected != tab)
          .allowsHitTesting(state.tabs.selected == tab)
          .accessibilityHidden(state.tabs.selected != tab)
          .animation(state.tabs.selected == tab ? Motion.swapIn : Motion.swapOut, value: state.tabs.selected)
      }
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
  }

  /// An existing pane in its scrolling column.
  ///
  /// - Parameter tab: The pane.
  /// - Returns: The pane's column.
  @ViewBuilder private func pane(_ tab: SettingsTab) -> some View {
    switch tab {
    case .general:
      FormColumn(insets: SettingsLayout.swapInsets) {
        GeneralPane(state: Binding(get: { state.general }, set: edit))
          .frame(maxWidth: SettingsLayout.contentMaximumWidth)
          .frame(maxWidth: .infinity)
      }
    case .helper:
      FormColumn(insets: SettingsLayout.swapInsets) {
        HelperPane(state: state.helper, send: send)
          .frame(maxWidth: SettingsLayout.contentMaximumWidth)
          .frame(maxWidth: .infinity)
      }
    case .agents:
      FormColumn(insets: SettingsLayout.swapInsets) {
        AgentsPane(state: state.agents, send: send)
          .frame(maxWidth: SettingsLayout.contentMaximumWidth)
          .frame(maxWidth: .infinity)
      }
    case .about:
      FormColumn(insets: SettingsLayout.aboutInsets) {
        AboutPane(state: state.about, send: send)
          .frame(maxWidth: SettingsLayout.contentMaximumWidth)
          .frame(maxWidth: .infinity)
      }
    }
  }

}
