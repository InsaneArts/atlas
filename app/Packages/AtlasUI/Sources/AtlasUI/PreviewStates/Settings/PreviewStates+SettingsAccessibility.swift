#if DEBUG
  import SwiftUI

  extension PreviewStates {
    /// Alert phases and inert keyboard focus over the stationary Settings shell.
    @MainActor static var settingsAccessibility: [PreviewState<SettingsState>] {
      var refused = settingsRecover
      refused.alert?.phase = .refused(message: "The helper is in use. Try again once it's free.")
      var done = settingsRecover
      done.alert?.phase = .done(title: "Recovered", message: "The helper is ready again.", button: "Done")
      var loggingOut = settingsLogOut
      loggingOut.alert?.phase = .running(label: "Logging out…")
      var forcingLogOut = settingsLogOutAnyway
      forcingLogOut.alert?.phase = .running(label: "Logging out…")
      var forgetting = settingsForget
      forgetting.destination = .projects
      forgetting.alert?.phase = .running(label: "Forgetting…")
      var installing = settingsInstall
      installing.alert?.phase = .running(label: "Installing…")
      var focus = settingsHelper
      focus.reveal = SettingsReveal(target: .recover, request: 1)
      return [
        PreviewState(name: "Recover refused", value: refused),
        PreviewState(name: "Recovered", value: done),
        PreviewState(name: "Logging out", value: loggingOut),
        PreviewState(name: "Logging out anyway", value: forcingLogOut),
        PreviewState(name: "Forgetting project", value: forgetting),
        PreviewState(name: "Installing update", value: installing),
        PreviewState(name: "Keyboard target focus", value: focus),
      ]
    }
  }

  #Preview("Reduced Motion") {
    @Previewable @State var state = PreviewStates.settingsHelper
    SettingsView(state: state, edit: { state.general = $0 }) { event in
      state = PreviewStates.settings(state, applying: event)
    }
    .environment(\._accessibilityReduceMotion, true)
    .frame(width: SettingsLayout.minimumWidth, height: SettingsLayout.minimumHeight)
  }

  #Preview("Reduced Transparency and Increased Contrast") {
    SettingsView(state: PreviewStates.settingsRecover, edit: { _ in }, send: { _ in })
      .environment(\._accessibilityReduceTransparency, true)
      .environment(\._colorSchemeContrast, .increased)
      .frame(width: SettingsLayout.minimumWidth, height: SettingsLayout.minimumHeight)
  }
#endif
