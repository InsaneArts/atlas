import SwiftUI

/// Startup and live links to the canonical everyday preferences.
struct GeneralPane: View {
  /// The shared preference values.
  let state: SettingsGeneralState
  /// Receives field edits and navigation.
  let send: @MainActor (SettingsEvent) -> Void

  /// The startup switch and two disclosure links.
  var body: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.sectionGap) {
      GroupedSection(
        [GeneralRow.openAtLogin],
        title: SettingsSearchCatalog.startup.section,
        description: SettingsSearchCatalog.startup.sectionDetail,
        notes: state.canOpenAtLogin ? [] : [.text("Open at login is unavailable in a development build.")]
      ) { _ in
        ListRow(
          SettingsSearchCatalog.startup.title,
          detail: SettingsSearchCatalog.startup.detail,
          trailing: .toggle(isOn: state.opensAtLogin),
          size: .compact
        ) { send(.setOpenAtLogin(!state.opensAtLogin)) }
        .disabled(!state.canOpenAtLogin)
        .settingsTarget(.startup)
      }
      GroupedSection(
        [GeneralRow.picture, .notify],
        title: SettingsSearchCatalog.everydayTitle,
        description: SettingsSearchCatalog.everydayDetail,
        notes: [.text("Closing Atlas's windows keeps it in the menu bar.")]
      ) { row in
        if row == .picture {
          SettingsDisclosureRow(
            title: "Screen & Viewer",
            summary: "Screen updates · \(state.pictureOptions.title(of: state.picture))"
          ) {
            send(.navigate(.root(.screen)))
          }
        } else {
          SettingsDisclosureRow(title: "Notifications", summary: state.notify ? "Notify me · On" : "Notify me · Off") {
            send(.navigate(.root(.notifications)))
          }
        }
      }
    }
  }
}
