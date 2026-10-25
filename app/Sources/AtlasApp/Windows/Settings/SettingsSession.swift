import AtlasUI

/// What the Settings window is doing: the pane shown, the alert asked, what runs, and what the last command said.
struct SettingsSession: Equatable {
  /// Visits during this window session.
  var navigation = SettingsNavigation(initial: .root(.general))
  /// The query and selection belong to this window session, never saved preferences.
  var search = SettingsSearchState()
  /// The latest explicit search-result reveal.
  var reveal: SettingsReveal?
  /// The existing content pane owned by the current destination.
  var tab: SettingsTab { navigation.destination?.root.pane ?? .general }
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
  /// The last explicit command failure or keyboard-selected search result for VoiceOver.
  var announcement: SettingsAnnouncement?
  /// Counts explicit announcements even when the previous one has been cleared.
  private var announcementRequests = 0
  /// Why each agent's last Connect or Disconnect failed, by id; cleared when status confirms the requested state.
  var connectFailures: [String: SettingsAgentFailure] = [:]

  /// Requests speech for an explicit result, even when its words repeat.
  ///
  /// - Parameter text: The words VoiceOver speaks.
  mutating func announce(_ text: String) {
    announcementRequests += 1
    announcement = SettingsAnnouncement(request: announcementRequests, text: text)
  }
}
