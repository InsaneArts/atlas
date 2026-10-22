/// A pane of the Settings window, as its tab row lists them.
package enum SettingsTab: String, CaseIterable, Hashable, Sendable {
  /// The app's own choices: login, the picture and when it needs the person.
  case general
  /// The helper: its account, whether it is ready to act, and what to do when something goes wrong.
  case helper
  /// The agents connected to Atlas and the projects taught.
  case agents
  /// The app's version, updates and the setup window.
  case about
}
