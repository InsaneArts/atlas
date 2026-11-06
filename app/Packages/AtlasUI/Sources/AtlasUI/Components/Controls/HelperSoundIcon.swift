import SwiftUI

/// The helper sound button's symbol: a speaker struck through while the person chose silence, sounding while not, in
/// the attention tint while the helper cannot make that choice hold. Each changes at once, drawn centred in its circle.
struct HelperSoundIcon: View {
  /// The button it draws for.
  let sound: HelperSoundButton

  /// The symbol, tinted when the sound is not as chosen.
  var body: some View {
    Image(systemName: sound.symbol.rawValue)
      .contentTransition(.identity)
      .foregroundStyle(sound.needsAttention ? AnyShapeStyle(Color.textAttention) : AnyShapeStyle(.foreground))
  }
}

#if DEBUG
  #Preview("Muted, on and unavailable") {
    HStack(spacing: Spacing.s) {
      ForEach(Array(PreviewStates.helperSound.enumerated()), id: \.offset) { _, sound in
        Button {
        } label: {
          HelperSoundIcon(sound: sound)
        }
        .buttonStyle(.capsule(.glyph, size: .small, outline: .circle))
        .help(sound.help)
      }
    }
    .padding()
    .sampleSurface()
  }
#endif
