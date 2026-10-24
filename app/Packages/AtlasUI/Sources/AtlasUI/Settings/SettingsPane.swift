import SwiftUI

/// The selected pane under the Settings window's header, each in a ``FormColumn`` that says how tall it draws. A new
/// pane crossfades with the last (v3 G2, X2): the old one out over 0.12 seconds, the new one in over 0.2 after 0.06;
/// under Reduce Motion both over 0.15. The window fits the new height meanwhile, in AppKit.
struct SettingsPane: View {
  /// What the window shows.
  let state: SettingsState
  /// Receives the General pane as the person changed it.
  let edit: @MainActor (SettingsGeneralState) -> Void
  /// Receives what the person does, and the pane's height.
  let send: @MainActor (SettingsEvent) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// What it draws, from its state.
  var body: some View {
    ZStack(alignment: .top) {
      pane(state.tabs.selected)
        .id(state.tabs.selected)
        .transition(transition)
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    .animation(reduceMotion ? Motion.crossfadeReduced : Motion.crossfadeIn, value: state.tabs.selected)
  }

  /// A pane in its column, at its insets, saying how tall it draws.
  ///
  /// - Parameter tab: The pane.
  /// - Returns: The pane's column.
  @ViewBuilder private func pane(_ tab: SettingsTab) -> some View {
    switch tab {
    case .general:
      FormColumn(insets: SettingsLayout.paneInsets, fits: fits(.general)) {
        GeneralPane(state: Binding(get: { state.general }, set: edit))
      }
    case .helper:
      FormColumn(insets: SettingsLayout.paneInsets, fits: fits(.helper)) {
        HelperPane(state: state.helper, send: send)
      }
    case .agents:
      FormColumn(insets: SettingsLayout.paneInsets, fits: fits(.agents)) {
        AgentsPane(state: state.agents, send: send)
      }
    case .about:
      FormColumn(insets: SettingsLayout.aboutInsets, fits: fits(.about)) {
        AboutPane(state: state.about, send: send)
      }
    }
  }

  /// Sends a pane's height as it draws.
  ///
  /// - Parameter tab: The pane.
  /// - Returns: What its column calls with its height.
  private func fits(_ tab: SettingsTab) -> @MainActor (CGFloat) -> Void {
    { height in send(.paneHeight(tab, height)) }
  }

  /// The crossfade: the old pane leaves quickly, the new one arrives just after it begins; one short fade each way
  /// under Reduce Motion.
  private var transition: AnyTransition {
    if reduceMotion {
      return AnyTransition.opacity
        .animation(Motion.crossfadeReduced)
    }
    return AnyTransition.asymmetric(
      insertion: AnyTransition.opacity
        .animation(Motion.crossfadeIn),
      removal: AnyTransition.opacity
        .animation(Motion.crossfadeOut)
    )
  }
}
