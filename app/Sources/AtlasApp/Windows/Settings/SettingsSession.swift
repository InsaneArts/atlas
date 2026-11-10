import AtlasUI

/// What the Settings window is doing: where it stands and has been, the sidebar's search, the alert asked, what runs,
/// and what the last command said.
struct SettingsSession: Equatable {
  /// The route shown, with Back's and Forward's.
  var navigation = SettingsNavigation()
  /// What the person typed in the sidebar's search.
  var query = ""
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
  /// Whether a change to the accounts list runs now.
  var accountsWorking = false
  /// Where a check for updates stands.
  var update = SettingsUpdate.idle
  /// What the last command said, on the page that ran it.
  var notice: SettingsNotice?
  /// Why each agent's last Connect or Disconnect failed, by id; cleared when status confirms the requested state.
  var connectFailures: [String: SettingsAgentFailure] = [:]
}
