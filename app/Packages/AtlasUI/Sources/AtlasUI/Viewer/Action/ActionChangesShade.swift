import SwiftUI

/// Show changes: a shade over every square that stayed the same between the pictures, so what changed stands out.
struct ActionChangesShade: View {
  /// The squares that changed, as shares of the picture.
  let changes: [CGRect]

  /// The shade, with the changed squares left clear.
  var body: some View {
    Canvas { context, size in
      var shade = Path(CGRect(origin: .zero, size: size))
      for change in changes {
        shade.addRect(
          CGRect(
            x: change.minX * size.width,
            y: change.minY * size.height,
            width: change.width * size.width,
            height: change.height * size.height
          )
        )
      }
      context.fill(shade, with: .color(Color.pictureUnchanged), style: FillStyle(eoFill: true))
    }
    .allowsHitTesting(false)
    .accessibilityHidden(true)
  }
}
