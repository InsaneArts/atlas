import SwiftUI

/// An app run's row, 32 high: its disclosure, the app's icon at 16, its name, what was done there, its problems in
/// orange, Replay while it is open, and its time with a blue dot while it runs. The summary gives way first, then the
/// name; the problems, Replay and the time never truncate. On a narrow card the summary hides and the problems stay.
struct ActivityRunRow: View {
  /// The run.
  let run: ViewerOutlineRun
  /// Its level in the outline.
  let level: Int
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.motionReduced) private var reduceMotion
  @Environment(\.outlineWidth) private var width

  /// The row, one button whether or not it can fold; held open, its press does nothing.
  var body: some View {
    Button {
      guard run.canFold else { return }
      withAnimation(ActivityMotion.fold(reduceMotion: reduceMotion)) {
        send(.toggleRun(run.id, opens: !run.isOpen))
      }
    } label: {
      label
    }
    .buttonStyle(OutlineRowStyle(height: ViewerLayout.runRowHeight))
    .layoutFrame("run \(run.id)")
    .accessibilityElement(children: .combine)
    .accessibilityLabel(run.label)
    .accessibilityValue(run.isOpen ? Text("Open") : Text("Folded"))
    .accessibilityHint(hint)
    .accessibilityRemoveTraits(run.canFold ? [] : .isButton)
  }

  /// What pressing it does, for VoiceOver; nothing while it is held open.
  private var hint: Text {
    guard run.canFold else { return Text(verbatim: "") }
    return run.isOpen ? Text("Folds this run") : Text("Shows this run's steps")
  }

  /// The disclosure, the icon, the words, Replay and the time.
  private var label: some View {
    HStack(spacing: .zero) {
      OutlineDisclosure(isOpen: run.isOpen)
        .opacity(run.canFold ? 1 : 0)
      ActivityGlyph(icon: run.icon, side: IconSize.s)
        .frame(width: ViewerLayout.outlineLead)
        .padding(.leading, ViewerLayout.outlineDisclosureGap)
        .padding(.trailing, ViewerLayout.outlineLeadGap)
      HStack(alignment: .firstTextBaseline, spacing: ViewerLayout.outlineTimeGap) {
        words
          .frame(maxWidth: .infinity, alignment: .leading)
        if run.isOpen, run.canReplay {
          ActivityReplayButton(run: run.id, send: send)
            .transition(.opacity)
        }
        OutlineTime(time: run.time, isRunning: run.isRunning)
      }
    }
    .padding(.leading, CGFloat(level) * ViewerLayout.outlineIndent)
    .lineLimit(1)
  }

  /// The name, the summary and the problems on one line.
  private var words: some View {
    HStack(alignment: .firstTextBaseline, spacing: .zero) {
      Text(verbatim: run.name)
        .font(Typography.font(.body, weight: .medium))
        .foregroundStyle(Color.textPrimary)
        .truncationMode(.tail)
        .layoutPriority(1)
      if width > ViewerLayout.outlineNarrow, !run.summary.isEmpty {
        Text(verbatim: run.summary)
          .font(Typography.digits(.callout))
          .foregroundStyle(Color.textSecondary)
          .truncationMode(.tail)
          .padding(.leading, ViewerLayout.runSummaryGap)
      }
      if !run.problems.isEmpty {
        Text(verbatim: "· \(run.problems)")
          .font(Typography.digits(.callout))
          .foregroundStyle(Color.textAttention)
          .fixedSize()
          .padding(.leading, ViewerLayout.runProblemGap)
          .layoutPriority(2)
      }
    }
  }
}
