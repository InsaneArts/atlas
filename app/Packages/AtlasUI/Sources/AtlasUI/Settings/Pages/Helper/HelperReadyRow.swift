import SwiftUI

/// One readiness check: its name, while it is not ready its fix in orange under it, and at its end where it stands, a
/// dot in its tone and its word, with Recover… before it where Recover is the fix. While Check Again reads, each mark
/// turns to a spinner and Checking at once, by a fade; as the reading lands each result pops in
/// ``SettingsLayout/checkStagger(index:)`` after the row's start, so the rows land one after another, top first.
/// VoiceOver hears Checking while it reads, never the result it is replacing.
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
      size: .compact,
      accessibilityChildren: row.offersRecover ? .contain : .combine
    ) {
      HStack(spacing: Spacing.m) {
        if row.offersRecover {
          Button("Recover…", action: recover)
            .buttonStyle(.capsule(.secondary, size: .row))
            .disabled(!buttons.canRecover)
            .help("Check the helper and start its mouse and keyboard again")
        }
        ZStack(alignment: .trailing) {
          mark
            .id(markKey)
            .transition(Motion.transition(landing, reduceMotion: reduceMotion))
        }
        .animation(markChange, value: markKey)
      }
    }
    .accessibilityValue(row.spokenValue)
  }

  /// Where the check stands: a spinner and Checking while Check Again reads, else its dot in its tone and its word.
  @ViewBuilder private var mark: some View {
    HStack(spacing: Spacing.xsPlus) {
      if row.isChecking {
        Spinner()
      } else {
        StatusDot(mark: row.item.state == .unknown ? .ring : .dot, tone: tone, size: .small)
      }
      Text(verbatim: row.spokenValue)
        .textStyle(.callout)
        .foregroundStyle(Color.textSecondary)
    }
    .accessibilityHidden(true)
  }

  /// The mark's identity: Checking, or the check's state.
  private var markKey: String { row.spokenValue }

  /// The dot's tone: ready, attention, or quiet for a check not run yet.
  private var tone: StatusTone {
    switch row.item.state {
    case .ready: .ready
    case .notReady: .attention
    case .unknown: .quiet
    }
  }

  /// How a new mark arrives: Checking fades in; a result pops in from small.
  private var landing: AnyTransition {
    guard !row.isChecking else { return .opacity }
    return AnyTransition.scale(scale: ListRowLeadLayout.landingScale)
      .combined(with: .opacity)
  }

  /// How the mark changes: to Checking at once, by a quick fade; to a result by a pop, later for each row down, or
  /// under Reduce Motion by a short fade, still staggered.
  private var markChange: Animation {
    guard !row.isChecking else { return Motion.markFill }
    return Motion.animation(Motion.pop, reduceMotion: reduceMotion, reduced: Motion.textSwap)
      .delay(SettingsLayout.checkStagger(index: row.index))
  }
}
