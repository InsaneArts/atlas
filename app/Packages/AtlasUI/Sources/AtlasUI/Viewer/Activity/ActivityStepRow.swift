import SwiftUI

/// One step of an opened run, or a folded run's problem line, on one 28-point line: its glyph in the lead column, what
/// was done, a picture button when it kept a picture of the screen just before it, and its time when the minute
/// changed. A screenshot's step shows its picture framed in place under its line; the action of unknown outcome offers
/// Show Screen and Recover… under it. Steps are not pressable, so they have no highlight.
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
          inset: ViewerLayout.pictureInPlaceInset,
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
      HStack(alignment: .center, spacing: ViewerLayout.pictureButtonGap) {
        words
        if step.notKept {
          notKept
        }
        if step.hasPicture {
          ActivityStepPictureButton { send(.openAction(step.id)) }
        }
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      if !step.time.isEmpty {
        OutlineTime(time: step.time)
      }
    }
    .padding(.leading, ViewerLayout.outlineText(level: level))
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
