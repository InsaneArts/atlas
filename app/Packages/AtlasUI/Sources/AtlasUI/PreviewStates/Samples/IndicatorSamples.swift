#if DEBUG
  import SwiftUI

  /// The indicators' samples, one component at a time.
  struct IndicatorSamples: View {
    /// Which component the samples show.
    enum Kind: Sendable {
      /// Spinners at the sizes in use.
      case spinner
      /// Discs at each size and tone in use.
      case disc
      /// Every mark, a breathing dot and the small dots a caption and a badge use.
      case statusDot
      /// The first, a middle and the last step of the track.
      case progressTrack
      /// Success and problem, settled.
      case heroDisc
      /// Each tile size, with a symbol and with characters.
      case symbolTile
      /// A status pill in each tone.
      case statusPill
    }

    private let kind: Kind

    /// Creates the samples of one component.
    ///
    /// - Parameter kind: Which component the samples show.
    init(_ kind: Kind) {
      self.kind = kind
    }

    /// That component's samples, on the ground.
    var body: some View {
      VStack(spacing: Spacing.m) {
        switch kind {
        case .spinner:
          HStack(spacing: Spacing.m) {
            Spinner(size: IconSize.xs)
            Spinner()
            Spinner(size: IconSize.xsPlus)
            Spinner(size: IconSize.sPlus)
          }
        case .disc:
          HStack(spacing: Spacing.m) {
            Disc(content: .symbol("exclamationmark"), tone: .attention, size: .mark)
            Disc(content: .number(1), tone: .waiting)
            Disc(content: .number(2), tone: .accent)
            Disc(content: .symbol("checkmark"), tone: .success)
            Disc(content: .letter(String("Y")), tone: .accentSoft)
            Disc(content: .letter(String("A")), tone: .neutral)
            Disc(content: .letter(String("T")), tone: .avatar, size: .avatar)
          }
        case .statusDot:
          HStack(spacing: Spacing.m) {
            StatusDot(mark: .dot, tone: .quiet)
            StatusDot(mark: .pulsing, tone: .active)
            StatusDot(mark: .dot, tone: .link)
            StatusDot(mark: .dot, tone: .attention)
            StatusDot(mark: .square, tone: .attention)
            StatusDot(mark: .small, tone: .quiet)
            StatusDot(mark: .pulsing, tone: .active, size: .small)
            StatusDot(mark: .small, tone: .ready)
          }
        case .progressTrack:
          ProgressTrack(index: 0, count: 6, label: String("Step 1 of 6"))
          ProgressTrack(index: 2, count: 6, label: String("Step 3 of 6"))
          ProgressTrack(index: 5, count: 6, label: String("Step 6 of 6"))
        case .heroDisc:
          HStack(spacing: Spacing.xl) {
            HeroDisc(kind: .success)
            HeroDisc(kind: .problem)
          }
        case .symbolTile:
          ForEach(SymbolTile.Size.allCases, id: \.self) { size in
            HStack(spacing: Spacing.m) {
              SymbolTile(icon: .symbol(SymbolName.click.rawValue), size: size)
              SymbolTile(icon: .text(String(">_")), size: size)
            }
          }
          HStack(spacing: Spacing.m) {
            ForEach(SymbolTile.Tone.allCases, id: \.self) { tone in
              SymbolTile(icon: .symbol(SymbolName.person.rawValue), size: .small, tone: tone)
            }
          }
        case .statusPill:
          HStack(spacing: Spacing.m) {
            StatusPill(label: String("Idle"), tone: .quiet)
            StatusPill(label: String("Ready"), tone: .ready)
            StatusPill(label: String("Working"), tone: .active)
            StatusPill(label: String("Needs you"), tone: .attention)
          }
        }
      }
      .sampleSurface()
    }
  }
#endif
