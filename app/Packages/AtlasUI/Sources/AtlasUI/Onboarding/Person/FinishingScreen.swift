import SwiftUI

/// The end-to-end check, once the person is back from the helper: one group of regular-weight checks with their words
/// on the right, each word in the one case and colour every window gives it. As the last check passes, its spinner
/// lands as a check, its word and the title swap, and the footer's Continue lights up a moment later. Continue, or Try
/// Again once it failed, is the footer's.
struct FinishingScreen: View {
  /// The checklist and how the check stands.
  let state: OnboardingFinishingScreenState
  @Environment(\.motionReduced) private var reduceMotion

  /// The checklist, then why it failed, in the status line's kept place.
  var body: some View {
    ScreenColumn {
      OnboardingHeader(label: "Welcome back", title: state.title, tail: state.tail, swapsTitle: true)
      GroupedList(Self.checks(state)) { check in
        ListRow(check.title, lead: check.lead, trailing: check.trailing, size: Self.rowSize)
      }
      .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced), value: state.live)
      .padding(.top, OnboardingLayout.Rhythm.paragraph)
      StatusSlot(line: state.failure.map { StatusLine(kind: .problem, text: $0) })
    }
  }

  /// The checklist's rows: 44 high at a setup list's inset, not a Settings group's.
  static let rowSize = ListRow.Size.checklist

  /// The four checks: the helper logged in, Atlas running there and its permissions, all passed by now, then the live
  /// check as it stands, its word green once it passed and orange once it failed.
  ///
  /// - Parameter state: The finishing check.
  /// - Returns: The checks, in order.
  static func checks(_ state: OnboardingFinishingScreenState) -> [ListRowItem<Int>] {
    [
      ListRowItem(id: 0, title: state.loggedIn, lead: .check(.done), trailing: .text("In the background")),
      ListRowItem(id: 1, title: "Atlas runs there", lead: .check(.done), trailing: .text("Starts by itself")),
      ListRowItem(
        id: 2,
        title: "Screen Recording and Accessibility",
        lead: .check(.done),
        trailing: .text(state.permissionsNote, tone: .success)
      ),
      ListRowItem(
        id: 3,
        title: "Mouse, keyboard and a test screenshot",
        lead: .check(state.live),
        trailing: .text(state.liveNote, tone: tone(state.live))
      ),
    ]
  }

  /// The live check's word's colour: green once it passed, orange once it failed, quiet while it runs.
  ///
  /// - Parameter live: How the live check stands.
  /// - Returns: The word's tone.
  private static func tone(_ live: CheckState) -> ListRow.NoteTone {
    switch live {
    case .done: .success
    case .failed: .problem
    case .busy: .quiet
    }
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
