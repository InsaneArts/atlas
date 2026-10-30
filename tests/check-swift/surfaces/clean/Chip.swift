import SwiftUI

/// A chip on an opaque fill. Never glass or a material, as `.glassEffect` would be.
struct Chip: View {
  var body: some View {
    Text(verbatim: "No .ultraThinMaterial here")
      .background {
        Capsule()
          .fill(Color.fillOverPicture)
      }
      .help("A magnifying glass looks at the picture")
  }
}
