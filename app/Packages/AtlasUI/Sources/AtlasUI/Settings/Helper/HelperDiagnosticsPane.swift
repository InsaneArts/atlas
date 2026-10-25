import SwiftUI

/// The complete data-driven readiness list and the shared diagnostics support action.
struct HelperDiagnosticsPane: View {
  /// The same rows, work state and notes used by Helper.
  let state: SettingsHelperState
  /// Receives the existing helper actions and diagnostics delegation.
  let send: @MainActor (SettingsEvent) -> Void

  /// All reported checks retain their order, guidance and eligible recovery action.
  var body: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.sectionGap) {
      VStack(alignment: .leading, spacing: SettingsLayout.titleGap) {
        HelperReadyHeader { send(.checkAgain) }
        GroupedSection(state.ready, notes: [state.readyNote?.sectionNote].compactMap { $0 }) { row in
          HelperReadyRow(row: row, buttons: state.buttons) { send(.askRecover) }
        }
      }
      .environment(\.marksSwapAtOnce, true)
      GroupedSection(
        [HelperAction.recover],
        title: "Support",
        description: "Copy Atlas's diagnostic report for support.",
        notes: [state.waiting.map { .status(.working, $0) }, state.notice?.sectionNote].compactMap { $0 }
      ) { _ in
        ListRow("Copy Diagnostics", trailing: .button("Copy Diagnostics"), size: .compact) { send(.copyDiagnostics) }
      }
    }
  }
}
