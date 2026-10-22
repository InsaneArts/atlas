import SwiftUI

/// The Helper pane: the helper's account with its status and Switch, whether it is ready to act with Check Again, and
/// Recover and Log Out for when something goes wrong.
struct HelperPane: View {
  /// What the pane shows.
  let state: SettingsHelperState
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.groupGap) {
      SettingsGroup {
        FormRow(
          title: state.account,
          detail: state.line,
          accessory: .status(state.status.mark, state.status.tone),
          spokenValue: state.status.label
        ) {
          Button("Switch to the Helper…") { send(.askSwitch) }
            .buttonStyle(.capsule(.glass, size: .mini))
            .disabled(!state.buttons.canSwitch)
            .help(state.buttons.switchHelp)
        }
      }
      SettingsGroup(title: "Ready to act") {
        SeparatedRows(items: state.ready) { row in
          FormRow(
            title: row.item.name,
            detail: row.item.text,
            accessory: .readiness(row.item.state),
            spokenValue: row.item.state.spokenValue
          ) {
            if row.offersCheck {
              Button("Check Again") { send(.checkAgain) }
                .buttonStyle(.capsule(.glass, size: .mini))
                .disabled(!state.buttons.canCheck)
                .help("Check the helper again now")
            }
          }
        }
      }
      SettingsGroup(title: "When something goes wrong", note: state.notice) {
        FormRow(title: "Recover", detail: "Checks the helper and starts its mouse and keyboard again") {
          Button("Recover…") { send(.askRecover) }
            .buttonStyle(.capsule(.glass, size: .mini))
            .disabled(!state.buttons.canRecover)
        }
        SeparatorLine()
        FormRow(title: "Log out the helper", detail: "Its apps quit. You log in to it again to use it") {
          Button("Log Out…") { send(.askLogOut) }
            .buttonStyle(.capsule(.glass, size: .mini))
            .disabled(!state.buttons.canLogOut)
        }
      }
    }
  }
}

#if DEBUG
  #Preview("Helper") {
    HelperPane(state: PreviewStates.settingsHelper.helper) { _ in }
      .frame(width: SettingsLayout.formWidth)
      .sampleSurface()
  }
#endif
