import SwiftUI

/// The top of the Settings window: the selected pane's name in the title bar, and the tabs under it, each its
/// symbol over its word. The name swaps in place by opacity, with the pane.
struct SettingsHeader: View {
  /// The selected pane's name.
  let title: String
  /// The tabs, with the pane shown selected.
  let tabs: CapsuleTabsState<SettingsTab>
  /// Receives the tab the person picks.
  let select: @MainActor (SettingsTab) -> Void

  /// What it draws, from its state.
  var body: some View {
    ZStack(alignment: .top) {
      Text(verbatim: title)
        .font(Typography.font(.body, weight: .semibold))
        .foregroundStyle(Color.textSecondary)
        .swapsText(on: title)
        .padding(.top, SettingsLayout.titleTop)
        // The window's own title names the pane for VoiceOver and the Window menu.
        .accessibilityHidden(true)
      Tabs(state: tabs, style: .symbolOverWord, send: select)
        .padding(.top, SettingsLayout.tabsTop)
    }
    .frame(maxWidth: .infinity)
    .frame(height: SettingsLayout.header, alignment: .top)
  }
}
