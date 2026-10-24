#if DEBUG
  import SwiftUI

  /// Samples for the modifiers, layouts and styles that have no view of their own to preview (U-12), each drawn as a
  /// window would use it.
  struct ModifierSamples: View {
    /// Which modifier the samples show.
    enum Kind: Sendable {
      /// A card on each tint.
      case cardSurface
      /// The rim around the picture, out of control and in it.
      case controlRim
      /// The picture in each look.
      case pictureTreatment
      /// The window's ground under a sample.
      case windowSurface
      /// The panel's card.
      case panelCard
      /// A window outlined near the picture's right edge, its label moved inside the outline.
      case outlineLabel
      /// A control at its own size and pressed.
      case pressScale
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
        case .cardSurface:
          ForEach([CardSurface.Tint.neutral, .accent, .attention], id: \.self) { tint in
            Text(verbatim: "\(tint)")
              .padding(Spacing.m)
              .frame(width: SampleWidth.narrow, alignment: .leading)
              .modifier(CardSurface(tint: tint, radius: Radius.large))
          }
        case .controlRim:
          HStack(spacing: Spacing.xl) {
            picture
              .controlRim(false, radius: Radius.large)
            picture
              .controlRim(true, radius: Radius.large)
          }
        case .pictureTreatment:
          HStack(spacing: Spacing.m) {
            ForEach([PictureLook.live, .dimmed, .paused], id: \.self) { look in
              picture
                .pictureTreatment(look)
            }
          }
        case .windowSurface:
          Text(verbatim: "On the window's ground")
            .padding(Spacing.xl)
            .windowSurface()
        case .panelCard:
          Text(verbatim: "The panel's card")
            .padding(Spacing.xl)
            .frame(width: SampleWidth.narrow)
            .panelCard()
        case .outlineLabel:
          ZStack {
            OutlineHighlight(frame: CGRect(x: 150, y: 20, width: 90, height: 100), dim: 0.36)
            OutlineLabelPlacement(outline: CGRect(x: 150, y: 20, width: 90, height: 100)) {
              GlassChip {
                Text(verbatim: "Opie Settings, a window near the edge")
              }
            }
          }
          .frame(width: SampleWidth.narrow, height: 140)
          .background(Color.fillPlaceholder)
        case .pressScale:
          HStack(spacing: Spacing.xl) {
            Capsule()
              .fill(Color.textPrimary)
              .frame(width: 120, height: ControlHeight.mPlus)
              .modifier(PressScale(isPressed: false, scale: 0.97))
            Capsule()
              .fill(Color.textPrimary)
              .frame(width: 120, height: ControlHeight.mPlus)
              .modifier(PressScale(isPressed: true, scale: 0.97))
          }
        }
      }
      .sampleSurface()
    }

    /// The sample desktop, small.
    @ViewBuilder private var picture: some View {
      if let desktop = SampleDesktop.picture {
        Image(decorative: desktop.image, scale: 1)
          .resizable()
          .frame(width: 160, height: 100)
          .clipShape(RoundedRectangle(cornerRadius: Radius.large))
      }
    }
  }
#endif
