import AppKit
import SwiftUI

/// A chip drawn on glass and a material.
struct Chip: View {
  var body: some View {
    VStack {
      Text(verbatim: "a")
        .glassEffect(.regular, in: Capsule())
      Button(String("b")) {}
        .buttonStyle(.glass)
      Text(verbatim: "c")
        .background(.ultraThinMaterial, in: Capsule())
      GlassEffectContainer {}
      Button(String("d")) {}
        .buttonStyle(.glassProminent)
    }
  }

  /// AppKit's glass and its material view.
  let views: [NSView.Type] = [NSGlassEffectView.self, NSVisualEffectView.self]
}
