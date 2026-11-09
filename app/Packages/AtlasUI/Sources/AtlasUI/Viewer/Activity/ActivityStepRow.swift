import SwiftUI

/// One step of an opened run, or a folded run's problem line, on one 28-point line: its glyph in the lead column, what
/// was done, a picture button when it kept a picture of the screen just before it, and its time when the minute
/// changed, with the listed account it acted in after its words where its run does not name it, and, where the
/// words fit whole beside it, a badge saying what let it act there. A screenshot's step shows its picture framed in
/// place under its line; the action of unknown outcome offers Show Screen and Recover… under it. Steps are not
/// pressable, so they have no highlight.
struct ActivityStepRow: View {
  /// The step.
  let step: ViewerActivityStep
  /// Its level in the outline: one under its run's.
  let level: Int
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The line, then its picture or its recovery.
  var body: some View {
    VStack(alignment: .leading, spacing: .zero) {
      line
      if step.offersRecovery {
        ActivityLineRecovery(send: send)
          .padding(.leading, ViewerLayout.outlineText(level: level))
          .padding(.bottom, ViewerLayout.stepsBelow)
      }
      if let shot = step.inPlace {
        ActivityPictureButton(
          tile: shot,
          size: ViewerLayout.pictureInPlace,
          send: send
        )
        .padding(.leading, ViewerLayout.outlineText(level: level))
        .padding(.bottom, ViewerLayout.stepsBelow)
      }
    }
    .accessibilityElement(children: .contain)
  }

  /// The glyph, the words and the picture button, and the time.
  private var line: some View {
    HStack(alignment: .firstTextBaseline, spacing: ViewerLayout.outlineTimeGap) {
      // The badge shows only where the words fit whole beside it; VoiceOver reads it either way.
      ViewThatFits(in: .horizontal) {
        details(showsBadge: true)
        details(showsBadge: false)
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      if !step.time.isEmpty {
        OutlineTime(time: step.time)
      }
    }
    .padding(.leading, ViewerLayout.outlineText(level: level))
    .padding(.trailing, ViewerLayout.outlineInset)
    .frame(height: ViewerLayout.stepRowHeight)
    .overlay(alignment: .leading) {
      glyph
        .frame(width: ViewerLayout.outlineLead)
        .padding(
          .leading,
          ViewerLayout.outlineText(level: level) - ViewerLayout.outlineLeadGap - ViewerLayout.outlineLead
        )
    }
    .accessibilityElement(children: .ignore)
    .accessibilityLabel(step.label)
    .takesFocusAfterSheet(isCurrent: step.isCurrent || step.returnsFocus, keys: false)
    .accessibilityActions {
      if step.hasPicture {
        Button("Show the screen before this step") { send(.openAction(step.id)) }
      }
      if let shot = step.inPlace {
        Button("Open the picture") { send(.openShot(shot.id)) }
      }
      if step.offersRecovery {
        Button("Show Screen") { send(.show(.screen)) }
        Button("Recover…") { send(.askRecover) }
      }
    }
  }

  /// What was done, the account its run does not name, Not kept, the badge when asked, and the picture button. The
  /// words give way first, then the account.
  ///
  /// - Parameter showsBadge: Whether what let it act in its account shows.
  /// - Returns: The line's details.
  private func details(showsBadge: Bool) -> some View {
    HStack(alignment: .center, spacing: ViewerLayout.pictureButtonGap) {
      words
      if let account = step.account {
        Text(verbatim: account)
          .font(Typography.font(.body))
          .foregroundStyle(Color.textSecondary)
          .lineLimit(1)
          .layoutPriority(1)
      }
      if step.notKept {
        notKept
      }
      if showsBadge, let allowance = step.allowance {
        AllowanceTag(badge: allowance)
      }
      if step.hasPicture {
        ActivityStepPictureButton { send(.openAction(step.id)) }
          .takesFocusAfterSheet(isCurrent: step.isCurrent || step.returnsFocus, voiceOver: false)
      }
    }
  }

  /// The step's glyph: its kind's symbol, blue while it runs, or the problem's mark.
  private var glyph: some View {
    let failed = step.mark == .failed
    return Image(systemName: failed ? SymbolName.attention.rawValue : step.kind.symbol.rawValue)
      .font(Typography.symbol(.row))
      .foregroundStyle(failed ? Color.attention : step.mark == .live ? Color.accent : Color.textSecondary)
      .frame(width: ViewerLayout.stepGlyph, height: ViewerLayout.stepGlyph)
      .animation(ActivityMotion.disc, value: step.mark)
      .accessibilityHidden(true)
  }

  /// What was done: a problem's words in orange then why in grey, else the step's words, semibold while it runs.
  @ViewBuilder private var words: some View {
    if let problem = step.problem {
      ActivityProblemWords(words: step.words, problem: problem, failed: step.mark == .failed)
    } else {
      ActivityStepWords(words: step.words, isLive: step.mark == .live)
    }
  }

  /// A small lock badge saying a password's text was not kept.
  private var notKept: some View {
    HStack(spacing: Spacing.xs) {
      Image(systemName: SymbolName.lock.rawValue)
        .font(Typography.font(.caption2, weight: .semibold))
      Text("Not kept")
        .font(Typography.font(.caption, weight: .semibold))
    }
    .foregroundStyle(Color.textSecondary)
    .padding(.horizontal, Spacing.s)
    .padding(.vertical, Stroke.regular)
    .background(Capsule().fill(Color.fillPill))
    .fixedSize()
    .help("Atlas never keeps what is typed into a password field")
  }
}
