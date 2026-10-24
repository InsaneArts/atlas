#if DEBUG
  import SwiftUI

  /// The buttons' samples: each capsule look, each size, capsules sharing a row, an alert's buttons, and the round
  /// buttons.
  struct CapsuleButtonSamples: View {
    /// Which samples to show.
    enum Kind: Sendable {
      /// Each look, then a disabled primary.
      case looks
      /// Mini, small, regular and large.
      case sizes
      /// Two capsules filling a row.
      case fill
      /// A plain alert's buttons side by side, then a destructive alert's, the default on the right.
      case alert
      /// The round buttons: regular, small, on, disabled.
      case circles
      /// Every look, the alert's too, side by side, as the component render draws them.
      case every
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
      VStack(spacing: Spacing.m) {
        switch kind {
        case .every:
          HStack(alignment: .top, spacing: Spacing.xxl) {
            CapsuleButtonSamples(.looks)
            CapsuleButtonSamples(.alert)
          }
        case .looks:
          Button(String("Continue")) {}
            .buttonStyle(.capsule())
          Button(String("Getting Started…")) {}
            .buttonStyle(.capsule(.glass))
          Button(String("Back")) {}
            .buttonStyle(.capsule(.plain))
          Button(String("Keys")) {}
            .buttonStyle(.capsule(.pressed))
          Button(String("Continue")) {}
            .buttonStyle(.capsule())
            .disabled(true)
        case .sizes:
          Button(String("Check Again")) {}
            .buttonStyle(.capsule(.glass, size: .mini))
          Button(String("Recover…")) {}
            .buttonStyle(.capsule(size: .small))
          Button(String("Recover")) {}
            .buttonStyle(.capsule())
          Button(String("Get Started")) {}
            .buttonStyle(.capsule(size: .large))
        case .fill:
          HStack(spacing: Spacing.s) {
            Button(String("Open Viewer")) {}
              .buttonStyle(.capsule(fill: true))
            Button(String("Switch to Helper…")) {}
              .buttonStyle(.capsule(.glass, fill: true))
          }
          .frame(width: SampleWidth.narrow)
        case .alert:
          VStack(spacing: Spacing.s) {
            HStack(spacing: Spacing.s) {
              Button(String("Not Now")) {}
                .buttonStyle(.capsule(.tinted, size: .compact, fill: true))
              Button(String("Recover")) {}
                .buttonStyle(.capsule(.accent, size: .compact, fill: true))
            }
            HStack(spacing: Spacing.s) {
              Button(String("Log Out"), role: .destructive) {}
                .buttonStyle(.capsule(.destructive, size: .compact, fill: true))
              Button(String("Cancel")) {}
                .buttonStyle(.capsule(.accent, size: .compact, fill: true))
            }
          }
          .frame(width: SampleWidth.alert)
        case .circles:
          HStack(spacing: Spacing.s) {
            Button(String("Pause"), systemImage: "pause.fill") {}
              .buttonStyle(.capsule(.glass, outline: .circle))
            Button(String("Sidebar"), systemImage: "sidebar.right") {}
              .buttonStyle(.capsule(.glass, size: .small, outline: .circle))
            Button(String("Sidebar"), systemImage: "sidebar.right") {}
              .buttonStyle(.capsule(.glass, size: .small, outline: .circle, isOn: true))
            Button(String("Pause"), systemImage: "pause.fill") {}
              .buttonStyle(.capsule(.glass, outline: .circle))
              .disabled(true)
          }
          .labelStyle(.iconOnly)
        }
      }
      .sampleSurface()
    }
  }
#endif
