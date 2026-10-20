import SwiftUI

/// The helper's desktop, letterboxed on a dark rounded stage, or why there is none.
struct PanelPicture: View {
  /// What the stage shows.
  enum Content: Equatable, Sendable {
    /// The desktop, live.
    case picture(DesktopPicture)
    /// No desktop, and why.
    case placeholder(String)
  }

  /// What the stage shows.
  let content: Content

  /// The stage, then the picture with its Live badge or the placeholder's words.
  var body: some View {
    ZStack(alignment: .bottomLeading) {
      RoundedRectangle(cornerRadius: Radius.medium)
        .fill(Color.stageBackground)
      switch content {
      case .picture(let picture):
        Image(decorative: picture.image, scale: 1)
          .resizable()
          .interpolation(.high)
          .aspectRatio(contentMode: .fit)
          .frame(maxWidth: .infinity, maxHeight: .infinity)
        LiveBadge()
          .padding(Spacing.m)
      case .placeholder(let text):
        Text(text)
          .font(Typography.font(.caption))
          .foregroundStyle(Color.textTertiary)
          .multilineTextAlignment(.center)
          .frame(maxWidth: .infinity, maxHeight: .infinity)
          .padding(Spacing.xxl)
      }
    }
    .aspectRatio(PanelLayout.pictureAspect, contentMode: .fit)
    .clipShape(RoundedRectangle(cornerRadius: Radius.medium))
    .overlay(
      RoundedRectangle(cornerRadius: Radius.medium)
        .strokeBorder(PanelLayout.pictureEdge, lineWidth: Stroke.hairline)
    )
    .accessibilityElement()
    .accessibilityLabel(spokenLabel)
  }

  /// What VoiceOver says for the stage: the picture by name, or the placeholder's words.
  private var spokenLabel: String {
    switch content {
    case .picture: "The helper's screen"
    case .placeholder(let text): text
    }
  }
}

#if DEBUG
  #Preview("No picture") {
    PanelPicture(content: .placeholder(String("The helper's screen is not available")))
      .frame(width: SampleWidth.narrow)
      .sampleSurface()
  }
#endif
