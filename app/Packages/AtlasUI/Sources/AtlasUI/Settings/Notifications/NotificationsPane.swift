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
      SettingsSectionHeading(title: "When Atlas needs you")
      GroupedSection([GeneralRow.notify]) { _ in
        ListRow(
          "Notify me",
          detail: "When an action may not have finished, or the helper logs out",
          trailing: .toggle(isOn: state.notify),
          size: .compact
        ) { send(.setNotify(!state.notify)) }
      }
    }
  }
}
