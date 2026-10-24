#if DEBUG
  import SwiftUI

  /// The floating chip's samples at each size: a notice, the chip at the top, and the mini screen's Live.
  struct FloatingChipSamples: View {
    var body: some View {
      VStack(spacing: Spacing.s) {
        FloatingChip {
          Text(verbatim: "Control released.")
        }
        FloatingChip(size: .top) {
          StatusDot(mark: .dot, tone: .active)
          Text(verbatim: "You have control")
            .fontWeight(.semibold)
          Text(verbatim: "Control-Shift-Escape gives it back")
            .fontWeight(.regular)
            .foregroundStyle(Color.textTertiary)
        }
        FloatingChip(size: .mini) {
          StatusDot(mark: .dot, tone: .active, size: .small)
          Text(verbatim: "Live")
        }
      }
      .sampleSurface()
    }
  }
#endif
