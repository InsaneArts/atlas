import SwiftUI

/// The keys pressed at once, as soft caps side by side, such as ⌘ and S.
struct KeyChord: View {
  /// The chord's caps, modifiers first.
  let keys: [String]

  /// The caps, 2 apart.
  var body: some View {
    HStack(spacing: Spacing.half) {
      ForEach(keys.indices, id: \.self) { index in
        Keycap(face: .text(keys[index]), style: .soft)
      }
    }
    .fixedSize()
  }
}

#if DEBUG
  #Preview("Samples") {
    KeycapSamples()
  }
#endif
