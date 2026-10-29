#if DEBUG
  import SwiftUI

  /// The chips on the picture: control with its keycaps, paused, and an action with its time; and the keycaps
  /// alone.
  struct PictureChipSamples: View {
    /// Which samples to show.
    enum Kind: Sendable {
      /// The control chip, the paused chip and an action chip, over the sample desktop.
      case chips
      /// The keycaps the control chip uses.
      case keycaps
    }

    private let kind: Kind

    /// Creates the samples of one kind.
    ///
    /// - Parameter kind: Which samples to show.
    init(_ kind: Kind) {
      self.kind = kind
    }

    /// The samples, on the ground.
    var body: some View {
      VStack(spacing: Spacing.s) {
        switch kind {
        case .chips:
          PictureChip(lead: .dot(.active)) {
            Text(verbatim: "You have control")
            Keycaps()
            Text(verbatim: "gives it back")
              .fontWeight(.regular)
              .foregroundStyle(Color.textSecondary)
          }
          PictureChip(lead: .symbol(.pause)) {
            Text(verbatim: "Picture paused")
            Text(verbatim: "Work in the helper goes on")
              .fontWeight(.regular)
              .foregroundStyle(Color.textSecondary)
          }
          PictureChip(lead: .dot(.active), time: String("just now")) {
            Text(verbatim: "Turning Show Center off in Opie")
          }
        case .keycaps:
          Keycaps()
        }
      }
      .sampleSurface()
    }
  }

  /// ⌃ ⇧ ⎋, the keys that give control back.
  private struct Keycaps: View {
    var body: some View {
      HStack(spacing: Spacing.half) {
        Keycap(face: .text(String("⌃")))
        Keycap(face: .text(String("⇧")))
        Keycap(face: .text(String("⎋")))
      }
    }
  }
#endif
