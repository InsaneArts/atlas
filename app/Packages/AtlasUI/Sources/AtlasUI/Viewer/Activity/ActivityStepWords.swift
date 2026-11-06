import SwiftUI

/// What a step did, in one line: the verb, what was typed in quotes, what it acted on in semibold, the keys pressed as
/// soft keycaps after the verb with what they went to after them, then a count of repeats. The step running now is all
/// semibold. The words change in place, never morphing one into the next, as a running step becomes one done.
struct ActivityStepWords: View {
  /// The words.
  let words: ViewerStepWords
  /// Whether the step runs now.
  var isLive = false
  /// The words' size: a step's line by default, larger as an action's title shown large.
  var style = TextStyle.body
  /// The words' weight while the step does not run.
  var weight = Font.Weight.regular

  /// The words, the keys, and what the keys went to.
  var body: some View {
    HStack(spacing: Spacing.xsPlus) {
      phrase(lead)
      if !words.keys.isEmpty {
        HStack(spacing: ViewerLayout.chordGap) {
          ForEach(words.keys.indices, id: \.self) { index in
            KeyChord(keys: words.keys[index])
          }
        }
        if !tail.isEmpty {
          phrase(tail)
        }
      }
    }
    .contentTransition(.identity)
  }

  /// The words before any keys: the verb, what was typed in quotes, the joiner, the target in semibold and the count;
  /// with keys, the verb alone.
  private var lead: [(text: Text, words: String)] {
    guard words.keys.isEmpty else { return [(Text(verbatim: words.verb), words.verb)] }
    return [
      (Text(verbatim: words.verb), words.verb),
      (Text(verbatim: words.quoted).foregroundStyle(Color.textSecondary), words.quoted),
      (Text(verbatim: words.joiner), words.quote.isEmpty ? "" : words.joiner),
      (Text(verbatim: words.target).fontWeight(.semibold), words.target),
      (Text(verbatim: words.count).foregroundStyle(Color.textSecondary), words.count),
    ]
  }

  /// The words after the keys: the key's name, the joiner, the target in semibold, and the count.
  private var tail: [(text: Text, words: String)] {
    [
      (Text(verbatim: words.keyName), words.keyName),
      (Text(verbatim: words.joiner), words.target.isEmpty ? "" : words.joiner),
      (Text(verbatim: words.target).fontWeight(.semibold), words.target),
      (Text(verbatim: words.count).foregroundStyle(Color.textSecondary), words.count),
    ]
    .filter { !$0.words.isEmpty }
  }

  /// Parts joined by spaces on one line, the empty ones left out.
  ///
  /// - Parameter parts: Each part, and its words, which say whether it shows.
  /// - Returns: The line.
  private func phrase(_ parts: [(text: Text, words: String)]) -> some View {
    let shown = parts.filter { !$0.words.isEmpty }
      .map(\.text)
    let line = shown.dropFirst()
      .reduce(shown.first ?? Text(verbatim: "")) { Text("\($0) \($1)") }
    return line.font(Typography.font(style, weight: isLive ? .semibold : weight))
      .foregroundStyle(Color.textPrimary)
      .lineLimit(1)
      .truncationMode(.tail)
  }
}
