import SwiftUI

/// An icon tab's label: its icon, an SF Symbol or the Orbit, in the secondary style, over its word.
struct IconTabLabel: View {
  /// The tab's word.
  let title: String
  /// Its icon.
  let icon: TabIcon

  var body: some View {
    VStack(spacing: Spacing.xs) {
      glyph
        .foregroundStyle(Color.textSecondary)
      Text(title)
        .font(Typography.font(.caption, weight: .medium))
        .lineLimit(1)
    }
  }

  /// The icon.
  @ViewBuilder private var glyph: some View {
    switch icon {
    case .symbol(let name):
      Image(systemName: name)
        .resizable()
        .scaledToFit()
        .frame(width: Layout.icon, height: Layout.icon)
        .accessibilityHidden(true)
    case .orbit:
      OrbitMark(height: Layout.orbit)
    }
  }
}

/// The label's own measures, from the board (mac.py `settings_window`).
private enum Layout {
  /// 20: a symbol's side.
  static let icon: CGFloat = 20
  /// 14: the Orbit's height, a little smaller than a symbol as its ring reaches wide.
  static let orbit: CGFloat = 14
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.xs) {
      Button {
      } label: {
        IconTabLabel(title: String("General"), icon: .symbol("gearshape"))
      }
      .buttonStyle(IconTabStyle(isSelected: true))
      Button {
      } label: {
        IconTabLabel(title: String("About"), icon: .orbit)
      }
      .buttonStyle(IconTabStyle(isSelected: false))
    }
    .sampleSurface()
  }
#endif
