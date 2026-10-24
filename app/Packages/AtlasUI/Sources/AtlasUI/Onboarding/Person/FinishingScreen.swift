import SwiftUI

/// The end-to-end check, once the person is back from the helper: one group of regular-weight checks with their notes
/// on the right (v3 D6). As the last check passes, its spinner lands as a check, its note and the title swap, and the
/// footer's Continue lights up a moment later (v3 S5). Continue, or Try Again once it failed, is the footer's.
struct FinishingScreen: View {
  /// The checklist and how the check stands.
  let state: OnboardingFinishingScreenState
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The checklist, then why it failed, or what the last check does.
  var body: some View {
    ScrollColumn(width: OnboardingLayout.column) {
      OnboardingHeader(label: "Welcome back", title: state.title, tail: state.tail, swapsTitle: true)
      GroupedList(Self.checks(state)) { check in
        ListRow(check.title, lead: check.lead, trailing: check.trailing, size: Self.rowSize)
      }
      .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced), value: state.live)
      .padding(.top, OnboardingLayout.Rhythm.paragraph)
      .pagePiece(.content)
      outcome
        .pagePiece(.status)
    }
  }

  /// Why the check failed, as the status under the checklist, or what the last check does, as a note.
  @ViewBuilder private var outcome: some View {
    if let failure = state.failure {
      StatusLine(kind: .problem, text: failure)
        .padding(.top, OnboardingLayout.Rhythm.content)
    } else {
      Note(text: "The last check starts input in the helper, takes a screenshot and confirms it.")
        .padding(.top, OnboardingLayout.Rhythm.status)
    }
  }

  /// The checklist's rows: 44 high at a setup list's inset (v3 `.row.check`), not a Settings group's.
  static let rowSize = ListRow.Size.checklist

  /// The four checks: the helper logged in, Atlas running there and its permissions, all passed by now, then the live
  /// check as it stands, its note orange once it failed.
  ///
  /// - Parameter state: The finishing check.
  /// - Returns: The checks, in order.
  static func checks(_ state: OnboardingFinishingScreenState) -> [ListRowItem<Int>] {
    [
      ListRowItem(id: 0, title: state.loggedIn, lead: .check(.done), trailing: .text("in the background")),
      ListRowItem(id: 1, title: "Atlas runs there", lead: .check(.done), trailing: .text("starts by itself")),
      ListRowItem(id: 2, title: "Screen Recording and Accessibility", lead: .check(.done), trailing: .text("allowed")),
      ListRowItem(
        id: 3,
        title: "Mouse, keyboard and a test screenshot",
        lead: .check(state.live),
        trailing: .text(state.liveNote, tone: state.live == .failed ? .problem : .quiet)
      ),
    ]
  }
}

#if DEBUG
  #Preview("Finishing") {
    FinishingScreen(state: PreviewStates.onboardingFinishingScreen)
      .onboardingSample()
  }

  #Preview("Finishing failed") {
    FinishingScreen(state: PreviewStates.onboardingFinishingFailedScreen)
      .onboardingSample()
  }
#endif
