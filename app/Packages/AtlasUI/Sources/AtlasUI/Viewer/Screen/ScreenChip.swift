import SwiftUI

/// The chip at the top of the screen, on glass (v3 V5, V11, V13): control and how to give it back, as keycaps; a paused
/// picture and that work goes on; or the text being typed, with a caret, which takes control's place in the same slot.
struct ScreenChip: View {
  /// Which chip.
  let chip: ViewerScreenChip

  /// The chip on glass.
  var body: some View {
    switch chip {
    case .typing(let text):
      GlassChip {
        Text(verbatim: "Typing")
          .fontWeight(.regular)
          .foregroundStyle(Color.textSecondary)
        Text(text)
          .fontWeight(.semibold)
        TypingCaret()
      }
    case .inControl:
      GlassChip(lead: .dot(.active)) {
        Text(verbatim: "You have control")
        HStack(spacing: ViewerLayout.keycapGap) {
          Keycap(face: .text("⌃"))
          Keycap(face: .text("⇧"))
          Keycap(face: .text("⎋"))
        }
        .accessibilityLabel(Text(verbatim: "Control-Shift-Escape"))
        Text(verbatim: "gives it back")
          .fontWeight(.regular)
          .foregroundStyle(Color.textSecondary)
      }
    case .paused:
      GlassChip(lead: .symbol(.pause)) {
        Text(verbatim: "Picture paused")
        Text(verbatim: "Work in the helper goes on")
          .fontWeight(.regular)
          .foregroundStyle(Color.textSecondary)
      }
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    VStack(spacing: Spacing.m) {
      ScreenChip(chip: .inControl)
      ScreenChip(chip: .paused)
      ScreenChip(chip: .typing(String("こんにちは")))
    }
    .sampleSurface()
  }
#endif
