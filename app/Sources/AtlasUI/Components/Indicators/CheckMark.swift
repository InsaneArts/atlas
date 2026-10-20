import SwiftUI

/// A green disc with a check.
struct CheckMark: View {
  /// The disc's side, in points.
  var size = IconSize.mark

  var body: some View {
    ZStack {
      Circle()
        .fill(Color.success)
      Image(systemName: "checkmark")
        .font(.system(size: size * Layout.glyphScale, weight: .bold))
        .foregroundStyle(Color.onSuccess)
    }
    .frame(width: size, height: size)
    .accessibilityHidden(true)
  }
}

/// The mark's own measures.
private enum Layout {
  /// 0.5: the check is half the disc, at every size.
  static let glyphScale: CGFloat = 0.5
}

#if DEBUG
  #Preview("Samples") {
    IndicatorSamples(.checkMark)
  }
#endif
