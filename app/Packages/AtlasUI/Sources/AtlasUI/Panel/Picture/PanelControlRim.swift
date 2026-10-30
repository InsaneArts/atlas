import SwiftUI

/// Control's edge round the panel's picture: a fine accent rim just outside it and a soft accent ring outside
/// that, settling in from a little larger; none out of control, and only a fade under Reduce Motion.
struct PanelControlRim: View {
  /// Whether the person holds control.
  let isShown: Bool
  @Environment(\.motionReduced) private var reduceMotion

  /// The rim and its ring, shown or faded away.
  var body: some View {
    ZStack {
      RoundedRectangle(cornerRadius: PanelLayout.ringCorner, style: .continuous)
        .strokeBorder(Color.accentRing, lineWidth: PanelLayout.ringWidth)
        .padding(-PanelLayout.ringReach)
      RoundedRectangle(cornerRadius: PanelLayout.rimCorner, style: .continuous)
        .strokeBorder(Color.accent, lineWidth: PanelLayout.rimWidth)
        .padding(-PanelLayout.rimReach)
    }
    .scaleEffect(isShown || reduceMotion ? 1 : PanelLayout.rimArrival)
    .opacity(isShown ? 1 : 0)
    .allowsHitTesting(false)
    .accessibilityHidden(true)
  }
}

#if DEBUG
  #Preview("Shown") {
    PanelScreen(picture: SampleDesktop.picture, isDimmed: false)
      .overlay { PanelControlRim(isShown: true) }
      .padding(Spacing.xxl)
      .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var isShown = true
    InteractivePreview {
      Toggle(String("In control"), isOn: $isShown)
    } content: {
      PanelScreen(picture: SampleDesktop.picture, isDimmed: false)
        .overlay { PanelControlRim(isShown: isShown) }
        .animation(PanelLayout.change, value: isShown)
        .padding(Spacing.xxl)
    }
  }
#endif
