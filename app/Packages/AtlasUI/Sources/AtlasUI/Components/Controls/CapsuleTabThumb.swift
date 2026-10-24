import SwiftUI

/// The selected tab's capsule: the pressed fill with a brighter edge.
struct CapsuleTabThumb: View {
  var body: some View {
    CapsuleEdge()
      .fill(Color.fillControlOn)
      .overlay(
        CapsuleEdge()
          .strokeBorder(Color.strokeSelected, lineWidth: Stroke.hairline)
      )
  }
}

#if DEBUG
  #Preview("Samples") {
    CapsuleTabThumb()
      .frame(width: IconSize.xxl, height: ControlHeight.s)
      .sampleSurface()
  }
#endif
