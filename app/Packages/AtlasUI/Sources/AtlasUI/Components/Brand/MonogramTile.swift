import SwiftUI

/// A tile drawn for an app whose own icon cannot be read, on a real icon's grid so it sits as one beside the others:
/// a body of 824 of the square's 1024 points, centred, with the same continuous corner, in its name's tint under a
/// soft light down its top, and the name's first letter in white. No edge and no shadow.
struct MonogramTile: View {
  /// The letter and its tint.
  let monogram: AppMonogram
  /// The square's side, as a real icon's whole canvas.
  var side = IconSize.xl

  /// The tile, centred in its square.
  var body: some View {
    let body = side * IconGrid.bodyShare
    let shape = RoundedRectangle(cornerRadius: body * IconGrid.cornerShare, style: .continuous)
    shape
      .fill(monogram.tint.color)
      .overlay(shape.fill(lift))
      .overlay(letter(body: body))
      .frame(width: body, height: body)
      .frame(width: side, height: side)
      .accessibilityHidden(true)
  }

  /// The light down the tile's top: ``Color/monogramLift`` at the top edge, gone by ``MonogramTileLayout/liftEnd`` of the way
  /// down, so the tint reads lit from above as an icon is.
  private var lift: LinearGradient {
    LinearGradient(
      stops: [
        Gradient.Stop(color: .monogramLift, location: .zero),
        Gradient.Stop(color: .clear, location: MonogramTileLayout.liftEnd),
      ],
      startPoint: .top,
      endPoint: .bottom
    )
  }

  /// The letter, semibold at half the body's side, centred.
  ///
  /// - Parameter body: The body's side.
  /// - Returns: The letter.
  private func letter(body: CGFloat) -> some View {
    Text(verbatim: monogram.letter)
      .font(Typography.font(size: body * MonogramTileLayout.letterShare, weight: .semibold))
      .foregroundStyle(Color.monogramLetter)
      .lineLimit(1)
      .minimumScaleFactor(MonogramTileLayout.letterShrink)
  }
}

#if DEBUG
  #Preview("Samples") {
    AppGlyphSamples(.tints)
  }
#endif
