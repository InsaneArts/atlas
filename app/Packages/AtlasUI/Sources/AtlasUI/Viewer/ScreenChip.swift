import SwiftUI

/// The chip at the top of the screen: what is being typed, control and how to give it back, or a paused picture.
struct ScreenChip: View {
  /// Which chip.
  let chip: ViewerScreenChip

  var body: some View {
    FloatingChip(size: .top) {
      switch chip {
      case .typing(let text):
        Text("Typing")
          .fontWeight(.regular)
          .foregroundStyle(Color.textTertiary)
        Text(text)
          .underline()
      case .inControl:
        StatusDot(mark: .dot, tone: .active, size: .small)
        Text("You have control")
          .fontWeight(.semibold)
        Text("Control-Shift-Escape gives it back")
          .fontWeight(.regular)
          .foregroundStyle(Color.textTertiary)
      case .paused:
        Image(systemName: "pause.fill")
          .font(Typography.font(.caption2))
        Text("Picture paused")
          .fontWeight(.semibold)
        Text("Work in the helper goes on")
          .fontWeight(.regular)
          .foregroundStyle(Color.textTertiary)
      }
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    VStack(spacing: Spacing.l) {
      ScreenChip(chip: .inControl)
      ScreenChip(chip: .paused)
      ScreenChip(chip: .typing(String("こんにちは")))
    }
    .sampleSurface()
  }
#endif
