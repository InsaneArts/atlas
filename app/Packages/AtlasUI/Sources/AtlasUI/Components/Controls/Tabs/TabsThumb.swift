import SwiftUI

/// The selected track tab's thumb: the held fill, a fine highlight along its top and a small shadow under it.
struct TabsThumb: View {
  /// The capsule, its top edge lit.
  var body: some View {
    Capsule()
      .fill(Color.fillControlOn)
      .overlay(alignment: .top) {
        RoundedRectangle(cornerRadius: TabsLayout.trackHeight / 2, style: .circular)
          .strokeBorder(Color.strokeHighlight, lineWidth: Stroke.hairline)
          .mask(alignment: .top) {
            Rectangle()
              .frame(height: Stroke.hairline)
          }
      }
      .shadow(color: Color.dropShadowFaint, radius: Layout.thumbShadow, y: Layout.thumbDrop)
  }
}

/// The thumb's own measures.
private enum Layout {
  /// 1.5: the thumb's shadow, half the equivalent 3-point CSS blur.
  static let thumbShadow: CGFloat = 1.5
  /// 1: how far the thumb's shadow falls.
  static let thumbDrop: CGFloat = 1
}

#if DEBUG
  #Preview("Thumb") {
    TabsThumb()
      .frame(width: IconSize.xxl, height: TabsLayout.trackHeight)
      .padding()
      .sampleSurface()
  }
#endif
