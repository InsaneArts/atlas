import SwiftUI

/// Small print.
struct Note: View {
  /// The words.
  let text: String
  /// How the print is painted: quiet for a remark, attention for a problem.
  var tone = TextTone.quiet

  var body: some View {
    Text(text)
      .font(Typography.font(.note))
      .foregroundStyle(tone.color)
      .multilineTextAlignment(.center)
      .fixedSize(horizontal: false, vertical: true)
  }
}

#if DEBUG
  #Preview("Samples") {
    TextSamples(.note)
  }
#endif
