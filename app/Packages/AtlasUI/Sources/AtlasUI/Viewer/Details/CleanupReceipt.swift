import SwiftUI

/// What the session's last cleanup did, above its button in Details: one row a step, what it did at 12 points with how
/// it went under it, such as "Done", "Kept: changed since it was installed" or "Couldn't: still in use. Try again.",
/// each with a hairline above it as a fact's.
struct CleanupReceipt: View {
  /// One fact a step, its name what the step did, its value how it went.
  let steps: [Fact]

  /// The group's name over its rows. Steps may share a name, as two copies of one app do, so each is told apart by
  /// its place.
  var body: some View {
    FactGroup("Last cleanup") {
      ForEach(Array(steps.enumerated()), id: \.offset) { _, step in
        row(step)
      }
    }
  }

  /// One step: what it did, then how it went under it, wrapping rather than cut short.
  ///
  /// - Parameter step: The step.
  /// - Returns: The row.
  private func row(_ step: Fact) -> some View {
    VStack(alignment: .leading, spacing: .zero) {
      Text(step.name)
        .foregroundStyle(Color.textPrimary)
      if !step.value.isEmpty {
        Text(step.value)
          .textStyle(.caption)
          .foregroundStyle(Color.textSecondary)
          .fixedSize(horizontal: false, vertical: true)
      }
    }
    .textStyle(.callout)
    .padding(.vertical, Layout.inset)
    .frame(maxWidth: .infinity, minHeight: ControlHeight.s, alignment: .leading)
    .overlay(alignment: .top) { SeparatorLine() }
    .accessibilityElement(children: .combine)
  }
}

/// The receipt's own measure.
private enum Layout {
  /// 6: above and below a row's text, as a fact's.
  static let inset = Spacing.xsPlus
}

#if DEBUG
  #Preview("Samples") {
    if let steps = PreviewStates.viewerSessionCleanedUp.details?.hostSession?.cleanup?.receipt {
      CleanupReceipt(steps: steps)
        .frame(width: ViewerLayout.detailsWidth)
        .sampleSurface()
    }
  }
#endif
