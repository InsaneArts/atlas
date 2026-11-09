#if DEBUG
  import SwiftUI

  /// Samples for the modifiers, layouts and styles that have no view of their own to preview, each drawn as a
  /// window would use it.
  struct ModifierSamples: View {
    /// Which modifier the samples show.
    enum Kind: Sendable {
      /// The window's ground under a sample.
      case windowSurface
      /// The panel's card.
      case panelCard
    }

    private let kind: Kind

    /// Creates the samples of one modifier.
    ///
    /// - Parameter kind: Which modifier the samples show.
    init(_ kind: Kind) {
      self.kind = kind
    }

    /// That modifier's samples, on the ground.
    var body: some View {
      VStack(spacing: Spacing.m) {
        switch kind {
        case .windowSurface:
          Text(verbatim: "On the window's ground")
            .padding(Spacing.xl)
            .windowSurface()
        case .panelCard:
          Text(verbatim: "The panel's card")
            .padding(Spacing.xl)
            .frame(width: SampleWidth.narrow)
            .panelCard()
        }
      }
      .sampleSurface()
    }
  }
#endif
