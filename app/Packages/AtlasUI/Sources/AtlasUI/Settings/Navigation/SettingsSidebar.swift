import SwiftUI

/// Stable sidebar navigation around the six root pages.
struct SettingsSidebar: View {
  /// Labels, current selection and live helper state.
  let state: SettingsState
  /// Receives a destination selection.
  let select: @MainActor (SettingsRoot) -> Void

  /// The reserved search field and independently scrollable destination clusters.
  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: SettingsLayout.sidebarClusterGap) {
        VStack(spacing: Spacing.m) {
          SearchField(text: "", prompt: "Search Settings", onChange: { _ in })
            .disabled(true)
          VStack(spacing: SettingsLayout.sidebarRowGap) {
            row(.helper)
          }
        }
        VStack(spacing: SettingsLayout.sidebarRowGap) {
          row(.general)
          row(.screen)
          row(.notifications)
        }
        row(.agents)
        row(.about)
      }
      .padding(.horizontal, SettingsLayout.sidebarInsets)
      .padding(.bottom, SettingsLayout.sidebarInsets)
      .padding(.top, SettingsLayout.sidebarTopInset)
    }
    .scrollBounceBehavior(.basedOnSize)
    .frame(width: SettingsLayout.sidebarWidth)
    .background(Color.fillGroup)
    .accessibilityLabel(Text(verbatim: state.tabs.label))
  }

  /// Draws one existing destination without creating a second label inventory.
  ///
  /// - Parameter pane: The destination to draw.
  /// - Returns: Its navigation row, when a label is present.
  private func row(_ pane: SettingsRoot) -> some View {
    let item = CapsuleTab(id: pane, title: SettingsDestination.root(pane).title, symbol: .gear)
    return SettingsSidebarRow(
      item: item,
      subtitle: pane == .helper ? state.helper.account.status.label : nil,
      selected: state.location.root == pane
    ) { select(pane) }
  }
}
