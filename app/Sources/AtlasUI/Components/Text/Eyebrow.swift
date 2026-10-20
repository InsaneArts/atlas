import SwiftUI

/// A small uppercase label above a title.
struct Eyebrow: View {
  /// The words, shown in capitals.
  let text: String
  /// How the label is painted: quiet for a section's name, attention when something needs the person.
  var tone = TextTone.quiet

  var body: some View {
    Text(text.uppercased())
      .font(Typography.font(.caption, weight: .semibold))
      .tracking(LetterSpacing.eyebrow)
      .foregroundStyle(tone.color)
  }
}

#if DEBUG
  #Preview("Samples") {
    TextSamples(.eyebrow)
  }
#endif
