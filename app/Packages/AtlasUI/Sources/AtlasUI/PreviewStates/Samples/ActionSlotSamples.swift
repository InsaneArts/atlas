#if DEBUG
  import SwiftUI

  /// The viewer's actions as v3 draws them (D11, V1): circles for symbols, glass capsules for words, one white capsule
  /// for the main action, and Keys and Stop in slots that show only while the person has control.
  struct ActionSlotSamples: View {
    /// Whether the person has control, so Keys and Stop show.
    let isInControl: Bool

    /// The actions in a row, on the ground.
    var body: some View {
      HStack(spacing: Spacing.s) {
        Button(String("Pause"), systemImage: SymbolName.pause.rawValue) {}
          .buttonStyle(.capsule(.glass, size: .small, outline: .circle))
        Button {
        } label: {
          HStack(spacing: Spacing.xsPlus) {
            Image(systemName: SymbolName.keyboard.rawValue)
              .font(Typography.symbol(.control))
            Text(verbatim: "Keys")
          }
        }
        .buttonStyle(.capsule(.glass, size: .small))
        .actionSlot(isInControl)
        Button(String("Stop")) {}
          .buttonStyle(.capsule(.glass, size: .small))
          .actionSlot(isInControl)
        Button(String("Take Control")) {}
          .buttonStyle(.capsule(size: .small))
        Button(String("More"), systemImage: SymbolName.more.rawValue) {}
          .buttonStyle(.capsule(.glass, size: .small, outline: .circle))
        Button(String("Details"), systemImage: SymbolName.sidebar.rawValue) {}
          .buttonStyle(.capsule(.glass, size: .small, outline: .circle))
      }
      .labelStyle(.iconOnly)
      .animatesActionSlots(isInControl)
      .sampleSurface()
    }
  }
#endif
