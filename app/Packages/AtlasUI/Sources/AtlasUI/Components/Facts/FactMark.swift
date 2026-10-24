import SwiftUI

/// A check's 14-point mark in an inspector: a green check when it passes, an orange round "!" when it fails,
/// an empty ring before it is checked.
struct FactMark: View {
  /// How the check stands.
  let state: ReadinessState

  /// The check, the round "!" or the empty ring.
  var body: some View {
    switch state {
    case .ready:
      OutcomeMark(kind: .done, side: Layout.side)
    case .notReady:
      OutcomeMark(kind: .problem, side: Layout.side)
    case .unknown:
      Circle()
        .strokeBorder(Color.strokeControl, lineWidth: Stroke.regular)
        .frame(width: Layout.side, height: Layout.side)
        .accessibilityHidden(true)
    }
  }
}

/// The mark's own measure.
private enum Layout {
  /// 14: the mark's side.
  static let side = IconSize.xsPlus
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.m) {
      FactMark(state: .ready)
      FactMark(state: .notReady)
      FactMark(state: .unknown)
    }
    .sampleSurface()
  }
#endif
