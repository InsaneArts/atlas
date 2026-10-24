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
      /// Rows with separators between them.
      case separatedRows
      /// The window's ground under a sample.
      case windowSurface
      /// The panel's card.
      case panelCard
      /// An outline's label near the picture's right edge, kept inside it.
      case outlineLabel
      /// A selected and an unselected icon tab.
      case iconTab
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
        case .separatedRows:
          CardGroup {
            SeparatedRows(items: Self.facts) { fact in
              DetailRow(name: fact.name, value: fact.value)
            }
          }
          .frame(width: SampleWidth.narrow)
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
          OutlineLabelPlacement(anchor: CGPoint(x: 220, y: 40)) {
            FloatingChip {
              Text(verbatim: "Opie Settings, a window near the edge")
            }
          }
          .frame(width: SampleWidth.narrow, height: 100)
          .background(Color.fillPlaceholder)
        case .iconTab:
          HStack(spacing: Spacing.xs) {
            Button {
            } label: {
              IconTabLabel(title: String("General"), icon: .symbol(SymbolName.gear.rawValue))
            }
            .buttonStyle(IconTabStyle(isSelected: true))
            Button {
            } label: {
              IconTabLabel(title: String("Helper"), icon: .symbol(SymbolName.display.rawValue))
            }
            .buttonStyle(IconTabStyle(isSelected: false))
          }
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

    /// Two facts, for the separated rows.
    private static let facts = [
      Fact(name: String("Helper"), value: String("Atlas")),
      Fact(name: String("Display"), value: String("1512 × 982")),
    ]

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
