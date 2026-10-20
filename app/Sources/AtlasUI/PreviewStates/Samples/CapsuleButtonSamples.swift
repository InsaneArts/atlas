#if DEBUG
  import SwiftUI

  /// The buttons' samples: each capsule look, each size, capsules sharing a row, and the round buttons.
  struct CapsuleButtonSamples: View {
    /// Which samples to show.
    enum Kind: Sendable {
      /// Each look, then a disabled primary.
      case looks
      /// Regular and large.
      case sizes
      /// Two capsules filling a row.
      case fill
      /// The round buttons: regular, small, on, disabled.
      case circles
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
      VStack(spacing: Spacing.l) {
        switch kind {
        case .looks:
          Button(String("Continue")) {}
            .buttonStyle(.capsule())
          Button(String("Choose Folder…")) {}
            .buttonStyle(.capsule(.glass))
          Button(String("Back")) {}
            .buttonStyle(.capsule(.plain))
          Button(String("Continue")) {}
            .buttonStyle(.capsule())
            .disabled(true)
        case .sizes:
          Button(String("Recover")) {}
            .buttonStyle(.capsule())
          Button(String("Get Started")) {}
            .buttonStyle(.capsule(size: .large))
        case .fill:
          HStack(spacing: Spacing.m) {
            Button(String("Open Viewer")) {}
              .buttonStyle(.capsule(fill: true))
            Button(String("Switch to Helper…")) {}
              .buttonStyle(.capsule(.glass, fill: true))
          }
          .frame(width: SampleWidth.narrow)
        case .circles:
          HStack(spacing: Spacing.m) {
            Button(String("Pause"), systemImage: "pause.fill") {}
              .buttonStyle(.circle())
            Button(String("Sidebar"), systemImage: "sidebar.right") {}
              .buttonStyle(.circle(.small))
            Button(String("Sidebar"), systemImage: "sidebar.right") {}
              .buttonStyle(.circle(.small, isOn: true))
            Button(String("Pause"), systemImage: "pause.fill") {}
              .buttonStyle(.circle())
              .disabled(true)
          }
          .labelStyle(.iconOnly)
        }
      }
      .sampleSurface()
    }
  }
#endif
