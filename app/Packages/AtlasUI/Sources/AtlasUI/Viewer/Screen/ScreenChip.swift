import SwiftUI

/// The chip at the top of the screen, on glass: control and how to give it back, as keycaps, Escape spelled out as the
/// system's shortcut displays spell it, so it never reads as a refresh arrow; a paused picture and that work goes on;
/// the text being typed, with a caret, which takes control's place in the same slot; or, while two or more host
/// sessions are live, the one that has the helper, its host's icon leading its name.
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
          Keycap(face: .text("esc"))
        }
        .accessibilityLabel(Text(verbatim: "Control-Shift-Escape"))
        Text(verbatim: "to give back")
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
    case .holder(let name, let icon):
      GlassChip {
        SessionIcon(icon: icon, side: ViewerLayout.sessionIcon)
        Text(name)
          .truncationMode(.middle)
        Text(verbatim: "has the helper")
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
      ScreenChip(chip: .holder(name: SessionSamples.claudeName, icon: SessionSamples.claudeIcon))
    }
    .sampleSurface()
  }
#endif
