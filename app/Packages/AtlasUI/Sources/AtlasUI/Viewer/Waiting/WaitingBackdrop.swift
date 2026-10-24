import SwiftUI

/// The last picture behind the waiting stage's words while the viewer connects again (v3 V7, X4): filling the stage,
/// blurred, its colour faded and its light dimmed, a little larger than the stage so the blur's soft edges fall
/// outside it. Only drawn, never aimed at.
///
/// The picture is first shrunk to a few dozen pixels and drawn back up smoothly, which blurs it on its own, so the
/// stage looks the same where SwiftUI's blur is not drawn, as in a render; the window blurs it further.
struct WaitingBackdrop: View {
  /// The last picture shown.
  let picture: DesktopPicture

  /// The softened picture, filling the stage.
  var body: some View {
    // A clear view takes the stage's size; the picture fills it at its own shape and is cut to it.
    Color.clear
      .overlay {
        Image(decorative: Self.softened(picture.image) ?? picture.image, scale: 1)
          .resizable()
          .interpolation(.high)
          .scaledToFill()
          .saturation(ViewerLayout.lastFrameSaturation)
          .blur(radius: ViewerLayout.lastFrameBlur, opaque: true)
          .overlay(
            Color.letterbox
              .opacity(ViewerLayout.lastFrameShade)
          )
          .scaleEffect(ViewerLayout.lastFrameScale)
      }
      .clipped()
      .accessibilityHidden(true)
  }

  /// The picture shrunk to ``Layout/sampleWidth`` pixels across at its own shape, which drawn back up to the stage's
  /// size reads as blurred.
  ///
  /// - Parameter image: The picture's pixels.
  /// - Returns: The shrunk picture; nil when it cannot be drawn.
  nonisolated static func softened(_ image: CGImage) -> CGImage? {
    guard image.width > 0, image.height > 0, let space = CGColorSpace(name: CGColorSpace.sRGB) else { return nil }
    let width = Layout.sampleWidth
    let height = max(1, Int((Double(image.height) * Double(width) / Double(image.width)).rounded()))
    guard
      let context = CGContext(
        data: nil,
        width: width,
        height: height,
        bitsPerComponent: Layout.bitsPerComponent,
        bytesPerRow: 0,
        space: space,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
      )
    else { return nil }
    context.interpolationQuality = .high
    context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
    return context.makeImage()
  }
}

/// The backdrop's own measures.
private enum Layout {
  /// 48: how many pixels across the shrunk picture keeps, about one for every 30 points of a wide stage, so drawn
  /// back up it blurs about as much as v3's 14-point blur.
  static let sampleWidth = 48
  /// 8: bits for each colour of a shrunk pixel.
  static let bitsPerComponent = 8
}

#if DEBUG
  #Preview("Samples") {
    if let picture = SampleDesktop.picture {
      WaitingBackdrop(picture: picture)
        .frame(width: SampleWidth.column * 2, height: SampleWidth.column)
        .sampleSurface()
    }
  }
#endif
