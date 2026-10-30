import SwiftUI

/// Control's edge on the screen: a 3-point accent line inside the picture's own shape while this window holds control,
/// fading in and out on ``Motion/fade``, which only changes opacity, so Reduce Motion keeps it. It never tints the
/// picture and takes no clicks, so the input surface under it keeps them.
struct ViewerControlEdge: View {
  /// Whether this window holds control.
  let isInControl: Bool
  /// The edge's corner, the screen's.
  var corner = ViewerLayout.screenRadius

  /// The edge, shown in control.
  var body: some View {
    RoundedRectangle(cornerRadius: corner, style: .continuous)
      .strokeBorder(Color.accent, lineWidth: ViewerLayout.controlEdge)
      .opacity(isInControl ? 1 : 0)
      .animation(Motion.fade, value: isInControl)
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
      RoundedRectangle(cornerRadius: ViewerLayout.screenRadius, style: .continuous)
        .fill(Color.fillControl)
        .frame(width: SampleWidth.column * 2, height: SampleWidth.column)
        .overlay { ViewerControlEdge(isInControl: isInControl) }
        .padding(Spacing.l)
    }
  }
#endif
