import SwiftUI

/// The outline's time column: right-aligned tabular figures in the tertiary grey, such as "01:08–now", with a small
/// blue dot before it while the run is under way.
struct OutlineTime: View {
  /// The time.
  let time: String
  /// Whether the run is under way.
  var isRunning = false

  /// The dot and the time, never truncated.
  var body: some View {
    HStack(spacing: ViewerLayout.runningDotGap) {
      if isRunning {
        Circle()
          .fill(Color.accent)
          .frame(width: ViewerLayout.runningDot, height: ViewerLayout.runningDot)
          .accessibilityLabel(Text("Running"))
      }
      Text(verbatim: time)
        .font(Typography.digits(.callout))
        .foregroundStyle(Color.textTertiary)
        .contentTransition(.identity)
    }
    .fixedSize()
  }
}
