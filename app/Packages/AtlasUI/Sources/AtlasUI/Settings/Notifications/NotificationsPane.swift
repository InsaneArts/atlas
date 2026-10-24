import SwiftUI

/// The existing notification preference in its own page.
struct NotificationsPane: View {
  /// The shared preference values.
  let state: SettingsGeneralState
  /// Receives only the edited notification field.
  let send: @MainActor (SettingsEvent) -> Void

  /// The one notification switch, with its existing explanation.
  var body: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.titleGap) {
      SettingsSectionHeading(title: SettingsSearchCatalog.notifications.section)
      GroupedSection([GeneralRow.notify]) { _ in
        ListRow(
          SettingsSearchCatalog.notifications.title,
          detail: SettingsSearchCatalog.notifications.detail,
          trailing: .toggle(isOn: state.notify),
          size: .compact
        ) { send(.setNotify(!state.notify)) }
        .settingsTarget(.notifications)
      }
    }
  }
}
