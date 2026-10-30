import SwiftUI

/// One readiness check: its mark, its name, and, while it is not ready, its fix in orange under it, with Recover… at
/// its end where Recover is the fix. While Check Again reads, each mark turns to a spinner at once, by a fade with no
/// scale; as the reading lands each result pops in ``SettingsLayout/checkStagger(index:)`` after the row's start, so
/// the rows land one after another, top first. VoiceOver hears Checking while it reads, never the result it is
/// replacing.
struct HelperReadyRow: View {
  /// The check.
  let row: SettingsReadyRow
  /// Which buttons can run now.
  let buttons: SettingsHelperButtons
  /// Asks to recover the helper.
  let recover: @MainActor () -> Void
  @Environment(\.motionReduced) private var reduceMotion

  /// What it draws, from its state.
  var body: some View {
    ListRowBody(
      row.item.name,
      detail: row.item.text.isEmpty ? nil : row.item.text,
      detailStyle: row.item.state == .notReady ? .problem : .plain,
      lead: lead,
      size: .compact,
      accessibilityChildren: row.offersRecover ? .contain : .combine
    ) {
      if row.offersRecover {
        Button("Recover…", action: recover)
          .buttonStyle(.capsule(.secondary, size: .mini))
          .disabled(!buttons.canRecover)
          .help("Check the helper and start its mouse and keyboard again")
      }
    }
    .accessibilityValue(row.spokenValue)
    .animation(markChange, value: lead)
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

  /// How the mark changes: to a spinner at once, by a quick fade; to a result by a pop, later for each row down, or
  /// under Reduce Motion by a short fade, still staggered.
  private var markChange: Animation {
    guard !row.isChecking else { return Motion.markFill }
    return Motion.animation(Motion.pop, reduceMotion: reduceMotion, reduced: Motion.textSwap)
      .delay(SettingsLayout.checkStagger(index: row.index))
  }
}
