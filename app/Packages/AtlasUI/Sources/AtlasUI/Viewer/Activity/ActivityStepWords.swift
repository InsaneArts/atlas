import SwiftUI

/// What a step did, in one line: the verb, what it acted on in semibold, the keys pressed as soft keycaps, then a
/// count of repeats. The step running now is all semibold. The words change in place, never morphing one into the
/// next, as a running step becomes one done.
struct ActivityStepWords: View {
  /// The words.
  let words: ViewerStepWords
  /// Whether the step runs now.
  var isLive = false

  /// The words and the keys.
  var body: some View {
    HStack(spacing: Spacing.xsPlus) {
      sentence
        .font(Typography.font(.body, weight: isLive ? .semibold : .regular))
        .foregroundStyle(Color.textPrimary)
        .lineLimit(1)
        .truncationMode(.tail)
      if !words.keys.isEmpty {
        HStack(spacing: ViewerLayout.chordGap) {
          ForEach(words.keys.indices, id: \.self) { index in
            KeyChord(keys: words.keys[index])
          }
        }
      }
      if !words.count.isEmpty, !words.keys.isEmpty {
        Text(verbatim: words.count)
          .font(Typography.digits(.body))
          .foregroundStyle(Color.textSecondary)
      }
    }
    .contentTransition(.identity)
  }

  /// The verb, the target in semibold, and the count when no keys stand between them.
  private var sentence: Text {
    let verb = Text(verbatim: words.verb)
    let target = Text(verbatim: words.target)
      .fontWeight(.semibold)
    let count = Text(verbatim: words.keys.isEmpty ? words.count : "")
      .foregroundStyle(Color.textSecondary)
    switch (words.target.isEmpty, words.count.isEmpty || !words.keys.isEmpty) {
    case (true, true): return verb
    case (false, true): return Text("\(verb) \(target)")
    case (true, false): return Text("\(verb) \(count)")
    case (false, false): return Text("\(verb) \(target) \(count)")
    }
  }
}
