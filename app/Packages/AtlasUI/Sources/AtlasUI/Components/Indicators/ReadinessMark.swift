import SwiftUI

/// A readiness check's small mark, as a row of checks sets it beside a 13-point name: a green check when ready, an
/// orange dot when not, an empty ring before it is checked.
struct ReadinessMark: View {
  /// How the check stands.
  let state: ReadinessState

  var body: some View {
    switch state {
    case .ready:
      CheckMark(size: Layout.side)
    case .notReady:
      Circle()
        .fill(Color.attention)
        .frame(width: Layout.side, height: Layout.side)
        .accessibilityHidden(true)
    case .unknown:
      Circle()
        .strokeBorder(Color.strokeStrong, lineWidth: Stroke.emphasis)
        .frame(width: Layout.side, height: Layout.side)
        .accessibilityHidden(true)
    }
  }
}

/// The mark's own measure, from the boards (mac.py `CHECK`).
private enum Layout {
  /// 14: the mark's side, the board's 13 (a 13-point name's height) rounded to an even point.
  static let side: CGFloat = 14
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.m) {
      ReadinessMark(state: .ready)
      ReadinessMark(state: .notReady)
      ReadinessMark(state: .unknown)
    }
    .sampleSurface()
  }
#endif
