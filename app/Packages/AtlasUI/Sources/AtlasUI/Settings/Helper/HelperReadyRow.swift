import SwiftUI

/// One readiness check (v3 G5): its mark, its name, and, while it is not ready, its fix in orange under it; the last
/// carries Check Again. While Check Again reads, each mark turns to a spinner, and as the reading lands each new mark
/// pops in ``SettingsLayout/checkStagger(index:)`` after the row's start, so the rows pass one after another. VoiceOver
/// hears Checking while it reads, never the result it is replacing.
struct HelperReadyRow: View {
  /// The check.
  let row: SettingsReadyRow
  /// Which buttons can run now.
  let buttons: SettingsHelperButtons
  /// Runs Check Again.
  let checkAgain: @MainActor () -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// What it draws, from its state.
  var body: some View {
    ListRowBody(
      row.item.name,
      detail: row.item.text.isEmpty ? nil : row.item.text,
      detailStyle: row.item.state == .notReady ? .problem : .plain,
      lead: lead,
      size: .compact,
      accessibilityChildren: row.offersCheck ? .contain : .combine
    ) {
      if row.offersCheck {
        Button(action: checkAgain) {
          WorkingLabel(title: "Check Again", isWorking: buttons.isChecking)
        }
        .buttonStyle(.capsule(.glass, size: .mini, isWorking: buttons.isChecking))
        .disabled(!buttons.canCheck)
        .help("Check the helper again now")
      }
    }
    .accessibilityValue(row.spokenValue)
    .animation(stagger, value: lead)
  }

  /// The mark: a spinner while Check Again reads, a green check when ready, an orange "!" when not, and an empty ring
  /// for a check not run yet.
  private var lead: ListRow.Lead {
    if row.isChecking { return .check(.busy) }
    switch row.item.state {
    case .ready: return .check(.done)
    case .notReady: return .check(.failed)
    case .unknown: return .pending
    }
  }

  /// The mark's pop, later for each row down; a plain fade, still staggered, under Reduce Motion.
  private var stagger: Animation {
    Motion.animation(Motion.pop, reduceMotion: reduceMotion, reduced: Motion.reduced)
      .delay(SettingsLayout.checkStagger(index: row.index))
  }
}
