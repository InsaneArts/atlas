import SwiftUI

/// Where an app's windows sit on the helper's display, drawn small: the display as a quiet tile in its own shape, and
/// each window a soft rectangle on it, the front one brightest and on top. Drawn from the frames the list read, so it
/// is never wrong and costs nothing.
struct AppScreenMap: View {
  /// The display's shape and the windows on it.
  let map: ViewerScreenMap

  /// The display and its windows.
  var body: some View {
    Canvas { context, size in
      let display = CGRect(origin: .zero, size: size)
      context.fill(
        Path(roundedRect: display, cornerRadius: ViewerLayout.appMapRadius, style: .continuous),
        with: .color(.fillControl)
      )
      for (index, share) in map.windows.enumerated()
        .reversed()
      {
        let window = CGRect(
          x: share.minX * size.width,
          y: share.minY * size.height,
          width: share.width * size.width,
          height: share.height * size.height
        )
        context.fill(
          Path(roundedRect: window, cornerRadius: ViewerLayout.appMapWindowRadius, style: .continuous),
          with: .color(index == 0 ? .fillTileRaised : .fillControlOn)
        )
      }
    }
    .aspectRatio(map.aspect, contentMode: .fit)
    .accessibilityHidden(true)
  }
}
