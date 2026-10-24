import SwiftUI

/// A badge that names every value: tokens, or its own documented Layout constants.
struct Badge: View {
  var body: some View {
    Text(verbatim: "3")
      .font(Typography.font(.caption, weight: .semibold))
      .padding(Spacing.m)
      .frame(width: Layout.side, height: Layout.side)
      .background(Color.accent)
      .opacity(Layout.dimmed)
      // A comment may say .frame(width: 20) or Color.red without a report.
      .accessibilityLabel(".padding(4) in a string is text")
      .overlay(
        Image(systemName: "star")
          .font(Typography.font(size: Layout.side * Layout.glyphScale, weight: .bold))
      )
  }
}

/// The badge's own measures.
private enum Layout {
  /// 22: the badge's side, sized to fit beside a row's text.
  static let side: CGFloat = 22
  /// 0.6: how far a muted badge dims.
  static let dimmed = 0.6
  /// 0.5: the star is half the badge.
  static let glyphScale: CGFloat = 0.5
  /// The preview's frame for comparing badge sizes.
  static let sample = CGSize(width: 120, height: 40)
}
