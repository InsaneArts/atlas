import SwiftUI

/// The General pane: opening at login and the picture, then when Atlas needs the person. Its values change through
/// the binding.
struct GeneralPane: View {
  /// The pane's values.
  @Binding var state: SettingsGeneralState

  var body: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.groupGap) {
      SettingsGroup {
        FormRow(title: "Open Atlas at login", detail: "Atlas sits in the menu bar from the start") {
          Toggle("Open Atlas at login", isOn: $state.opensAtLogin)
            .toggleStyle(.accentSwitch)
            .controlSize(.small)
            .disabled(!state.canOpenAtLogin)
        }
        SeparatorLine()
        FormRow(title: "Picture", detail: "How often Atlas's windows show the helper's screen") {
          PopupButton(label: "Picture", options: state.pictureOptions, selection: $state.picture)
        }
      }
      SettingsGroup(title: "When Atlas needs you") {
        FormRow(title: "Notify me", detail: "When an action may not have finished, or the helper logs out") {
          Toggle("Notify me", isOn: $state.notify)
            .toggleStyle(.accentSwitch)
            .controlSize(.small)
        }
        SeparatorLine()
        FormRow(title: "Show a badge on the Dock icon") {
          Toggle("Show a badge on the Dock icon", isOn: $state.badge)
            .toggleStyle(.accentSwitch)
            .controlSize(.small)
        }
      }
    }
  }
}

#if DEBUG
  #Preview("General") {
    GeneralPane(state: .constant(PreviewStates.settingsGeneral.general))
      .frame(width: SettingsLayout.formWidth)
      .sampleSurface()
  }
#endif
