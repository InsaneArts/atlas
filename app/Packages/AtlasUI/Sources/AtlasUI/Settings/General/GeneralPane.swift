import SwiftUI

/// The General pane: opening at login and how often the screen updates, then when Atlas needs the person. Its values
/// change through the binding.
struct GeneralPane: View {
  /// The pane's values.
  @Binding var state: SettingsGeneralState

  /// What it draws, from its state.
  var body: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.sectionGap) {
      GroupedSection(GeneralRow.start) { row in
        self.row(row)
      }
      GroupedSection(GeneralRow.needs, title: "When Atlas needs you") { row in
        self.row(row)
      }
    }
  }

  /// One row: its words and its control.
  ///
  /// - Parameter row: The row.
  /// - Returns: The row.
  @ViewBuilder private func row(_ row: GeneralRow) -> some View {
    switch row {
    case .openAtLogin:
      ListRow(
        "Open Atlas at login",
        detail: "Atlas sits in the menu bar from the start",
        trailing: .toggle(isOn: state.opensAtLogin),
        size: .compact
      ) { state.opensAtLogin.toggle() }
      .disabled(!state.canOpenAtLogin)
    case .picture:
      ListRowBody("Screen updates", detail: "How often Atlas's windows show the helper's screen", size: .compact) {
        PopupButton(label: "Screen updates", options: state.pictureOptions, selection: $state.picture)
      }
    case .notify:
      ListRow(
        "Notify me",
        detail: "When an action may not have finished, or the helper logs out",
        trailing: .toggle(isOn: state.notify),
        size: .compact
      ) { state.notify.toggle() }
    }
  }
}

#if DEBUG
  #Preview("General") {
    FormColumn(insets: SettingsLayout.swapInsets) {
      GeneralPane(state: .constant(PreviewStates.settingsGeneral.general))
    }
    .frame(width: SettingsLayout.width)
    .sampleSurface()
  }
#endif
