import SwiftUI

/// The panel's picture frame with no screen to show: Atlas's quiet orbit on the empty stage, over one line saying why
/// or what helps. VoiceOver reads the line.
struct PanelEmptyPicture: View {
  /// The line, such as "The helper is logged out".
  let line: String

  var body: some View {
    VStack(spacing: Spacing.sPlus) {
      OrbitMark(height: PanelLayout.emptyOrbit)
      Text(line)
        .font(Typography.font(.callout))
        .multilineTextAlignment(.center)
        .fixedSize(horizontal: false, vertical: true)
    }
    .foregroundStyle(Color.textTertiary)
    .padding(.horizontal, PanelLayout.emptyInset)
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .background {
      RoundedRectangle(cornerRadius: Radius.medium)
        .fill(Color.fillPlaceholder)
      RoundedRectangle(cornerRadius: Radius.medium)
        .strokeBorder(Color.separator, lineWidth: Stroke.hairline)
    }
    .accessibilityElement(children: .combine)
  }
}

#if DEBUG
  #Preview("Not ready") {
    PanelEmptyPicture(line: String("Recover checks the helper and starts it again"))
      .frame(width: PanelLayout.picture.width, height: PanelLayout.picture.height)
      .sampleSurface()
  }
#endif
