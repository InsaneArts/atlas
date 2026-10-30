import SwiftUI

#if DEBUG
  /// ⌃ ⇧ ⎋, the keys that give control back.
  struct Keycaps: View {
    /// The component drawn in its current state.
    var body: some View {
      HStack(spacing: Spacing.half) {
        Keycap(face: .text(String("⌃")))
        Keycap(face: .text(String("⇧")))
        Keycap(face: .text(String("⎋")))
      }
    }
  }
#endif
