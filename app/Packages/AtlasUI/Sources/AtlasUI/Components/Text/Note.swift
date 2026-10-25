import SwiftUI

/// Small print.
struct Note: View {
  /// The words.
  let text: String
  /// How the print is painted: quiet for a remark, attention for a problem.
  var tone = TextTone.quiet
  /// How its lines line up: centred by default, as under a screen's content.
  var alignment = TextAlignment.center

  /// The small print in its tone.
  var body: some View {
    Text(text)
      .font(Typography.font(.caption))
      .foregroundStyle(tone.color)
      .multilineTextAlignment(alignment)
      .fixedSize(horizontal: false, vertical: true)
  }
}

#if DEBUG
  #Preview("Samples") {
    TextSamples(.note)
  }
#endif
