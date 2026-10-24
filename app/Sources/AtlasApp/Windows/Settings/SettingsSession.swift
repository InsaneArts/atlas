import AtlasUI

/// What the Settings window is doing: the pane shown, the alert asked, what runs, and what the last command said.
struct SettingsSession: Equatable {
  /// The pane shown.
  var tab = SettingsTab.general
  /// The alert asked, if any.
  var alert: SettingsAlert?
  /// Whether the alert's command runs now, so its buttons wait.
  var alertRunning = false
  /// Why the alert's Recover was refused, shown in the alert until it is answered; nil for none.
  var alertRefusal: String?
  /// Whether the alert's Recover worked and the alert holds, saying Recovered, before it closes.
  var alertDone = false
  /// Whether Check Again's reading runs now.
  var checking = false
  /// Whether Atlas closes a login now, under the person's approval.
  var closing = false
  /// Where a check for updates stands.
  var update = SettingsUpdate.idle
  /// What the last command said, in the pane that ran it.
  var notice: SettingsNotice?
  /// Why each agent's last Connect failed, by the agent's id, as the command said it; an agent's entry goes once it
  /// is connected again.
  var connectFailures: [String: String] = [:]
}
