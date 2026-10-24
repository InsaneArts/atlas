import SwiftUI

/// Why Take Control was just refused, on glass at the top of the stage (v3 V12): an orange mark and the reason in
/// orange. The window brings it in and the controller takes it away after 2.4 seconds.
struct ViewerToastChip: View {
  /// The reason.
  let text: String

  /// The mark and the reason on glass.
  var body: some View {
    GlassChip {
      Image(systemName: SymbolName.attention.rawValue)
        .font(Typography.symbol(.row, weight: .medium))
        .foregroundStyle(Color.attention)
        .accessibilityHidden(true)
      Text(text)
        .foregroundStyle(Color.textAttention)
    }
  }
}

#if DEBUG
  #Preview("Mid-action") {
    ViewerToastChip(text: "The agent is mid-action. Try again in a moment.")
      .sampleSurface()
  }
#endif
