import SwiftUI

/// The few latest actions, oldest first, stacked in the screen's corner: a failure marked orange, the newest with a
/// breathing blue dot, older ones faded.
struct CaptionStack: View {
  /// The actions, oldest first.
  let entries: [ActivityEntry]

  var body: some View {
    VStack(alignment: .leading, spacing: Spacing.xsPlus) {
      // An entry's id is its place in the service's list of recent actions, the one identity the service gives.
      ForEach(entries) { entry in
        FloatingChip {
          switch entry.captionMark {
          case .failed:
            StatusDot(mark: .dot, tone: .attention, size: .small)
          case .newest:
            StatusDot(mark: .pulsing, tone: .active, size: .small)
          case nil:
            EmptyView()
          }
          Text(entry.line)
          Text(entry.when)
            .fontWeight(.regular)
            .foregroundStyle(Color.textTertiary)
        }
        .opacity(entry.isNewest ? 1 : Layout.older)
      }
    }
    .accessibilityElement(children: .contain)
    .accessibilityLabel("Latest actions")
  }
}

/// The stack's own measures.
private enum Layout {
  /// 0.6: a caption older than the newest (v3.py `chip`, faded).
  static let older = 0.6
}

#if DEBUG
  #Preview("Samples") {
    CaptionStackSamples()
  }
#endif
