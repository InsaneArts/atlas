import SwiftUI

/// Control's edge round the picture: a 2-point accent rim 2 points outside it, the ground showing between,
/// and a 4-point soft accent ring outside that. It settles in from a little larger on ``Motion/state`` while
/// this window holds control and goes the same way; under Reduce Motion it only fades. It never tints the
/// picture.
struct ViewerControlRim: View {
  /// Whether this window holds control.
  let isInControl: Bool
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The ring outside the rim, and the rim, shown in control.
  var body: some View {
    ZStack {
      RoundedRectangle(cornerRadius: ViewerLayout.rimRadius + ViewerLayout.rimWidth + ViewerLayout.ringWidth)
        .strokeBorder(Color.accentRing, lineWidth: ViewerLayout.ringWidth)
        .padding(-(ViewerLayout.rimGap + ViewerLayout.rimWidth + ViewerLayout.ringWidth))
      RoundedRectangle(cornerRadius: ViewerLayout.rimRadius + ViewerLayout.rimWidth)
        .strokeBorder(Color.accent, lineWidth: ViewerLayout.rimWidth)
        .padding(-(ViewerLayout.rimGap + ViewerLayout.rimWidth))
    }
    .opacity(isInControl ? 1 : 0)
    .scaleEffect(ViewerLayout.rimScale(isInControl: isInControl, reduceMotion: reduceMotion))
    .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced), value: isInControl)
    .allowsHitTesting(false)
    .accessibilityHidden(true)
  }
}

#if DEBUG
  #Preview("Interactive") {
    @Previewable @State var isInControl = true
    InteractivePreview {
      Toggle(String("In control"), isOn: $isInControl)
    } content: {
      RoundedRectangle(cornerRadius: Radius.large)
        .fill(Color.fillControl)
        .frame(width: SampleWidth.column * 2, height: SampleWidth.column)
        .overlay { ViewerControlRim(isInControl: isInControl) }
        .padding(Spacing.l)
    }
  }
#endif
