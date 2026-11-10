import AtlasStatus
import AtlasUI

extension SettingsPresenter {
  /// The sidebar: a row for every page, Helper's saying how the helper stands with its dot's tone, and the search.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The sidebar.
  static func sidebar(_ snapshot: SettingsSnapshot) -> SettingsSidebarState {
    let helper = helperStanding(snapshot.facts)
    let rows = SettingsPage.allCases
      .map { page in
        page == .helper
          ? SettingsSidebarRow(page: page, title: SettingsWords.page(page), subtitle: helper.word, tone: helper.tone)
          : SettingsSidebarRow(page: page, title: SettingsWords.page(page))
      }
    return SettingsSidebarState(rows: rows, query: snapshot.session.query)
  }

  /// How the helper stands, as the Helper row says it under its title, as the main window's helper line titles it:
  /// Connecting while Atlas waits for the service, Logged out, Needs you while it needs recovering, Needs setup while
  /// a readiness check fails or it is not ready, else Ready.
  ///
  /// - Parameter facts: What the rest of the app says now.
  /// - Returns: The word and its dot's tone.
  static func helperStanding(_ facts: SettingsFacts) -> (word: String, tone: StatusTone) {
    let overview = facts.status.overview
    let mode = facts.connecting ? Mode.connecting : Mode.of(overview)
    let failing = StatusReading(facts.status)
      .readinessItems
      .contains { $0.state == .notReady }
    switch mode.kind {
    case .connecting: return (SettingsWords.helperConnecting, .quiet)
    case .loggedOut: return (SettingsWords.helperLoggedOut, .attention)
    case .needsYou: return (SettingsWords.helperNeedsYou, .attention)
    case .notReady: return (SettingsWords.helperNeedsSetup, .attention)
    case .idle, .working, .control, .stopped, .inUseElsewhere:
      return failing ? (SettingsWords.helperNeedsSetup, .attention) : (SettingsWords.helperReady, .ready)
    }
  }

  /// How the helper stands as a status: its word and tone, with a dot.
  ///
  /// - Parameter facts: What the rest of the app says now.
  /// - Returns: The status.
  static func standing(_ facts: SettingsFacts) -> StatusSummary {
    let standing = helperStanding(facts)
    return StatusSummary(label: standing.word, tone: standing.tone, mark: .dot)
  }
}
