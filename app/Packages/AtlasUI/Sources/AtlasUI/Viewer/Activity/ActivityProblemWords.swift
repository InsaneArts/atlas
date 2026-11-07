import SwiftUI

/// A step that did not work, on one line: what it could not do as a sentence in semibold orange, such as "Couldn't
/// press Go.", then why and what came next in grey; one refused before anything was sent keeps its words quiet.
struct ActivityProblemWords: View {
  /// What it tried.
  let words: ViewerStepWords
  /// Why it did not work and what came next.
  let problem: ViewerActivityProblem
  /// Whether it failed, rather than being refused before anything was sent.
  let failed: Bool

  /// The words and the reason, tail-truncated together.
  var body: some View {
    let said = Text(verbatim: sentence)
      .font(Typography.font(.body, weight: failed ? .semibold : .regular))
      .foregroundStyle(failed ? Color.textAttention : Color.textPrimary)
    let why = Text(verbatim: problem.detail.isEmpty ? "" : " \(problem.detail)")
      .font(Typography.font(.body))
      .foregroundStyle(Color.textSecondary)
    return Text("\(said)\(why)")
      .lineLimit(1)
      .truncationMode(.tail)
  }

  /// The words as a sentence, with its full stop.
  private var sentence: String {
    let text = words.text
    return text.hasSuffix(".") ? text : "\(text)."
  }
}
