#if DEBUG
  import SwiftUI

  /// The indicators' samples, one component at a time.
  struct IndicatorSamples: View {
    /// Which component the samples show.
    enum Kind: Sendable {
      /// Spinners at the sizes in use.
      case spinner
      /// Check marks at the sizes in use.
      case checkMark
      /// Discs at each size and tone in use.
      case disc
      /// A still and a breathing dot at each size.
      case statusDot
      /// The first, a middle and the last step.
      case pageDots
      /// Waiting and done.
      case progressPill
      /// One badge per tone.
      case statusBadge
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
      VStack(spacing: Spacing.l) {
        switch kind {
        case .spinner:
          HStack(spacing: Spacing.l) {
            Spinner(size: IconSize.glyph)
            Spinner()
            Spinner(size: IconSize.glyphLarge)
            Spinner(size: IconSize.mark)
          }
        case .checkMark:
          HStack(spacing: Spacing.l) {
            CheckMark()
            CheckMark(size: IconSize.disc)
            CheckMark(size: IconSize.hero)
          }
        case .disc:
          HStack(spacing: Spacing.l) {
            Disc(content: .symbol("exclamationmark"), tone: .attention, size: .mark)
            Disc(content: .number(1), tone: .waiting)
            Disc(content: .number(2), tone: .accent)
            Disc(content: .symbol("checkmark"), tone: .success)
            Disc(content: .letter(String("Y")), tone: .accentSoft)
            Disc(content: .letter(String("A")), tone: .neutral)
            Disc(content: .letter(String("T")), tone: .avatar, size: .avatar)
            Disc(content: .letter(String("I")), tone: .avatar, size: .large)
          }
        case .statusDot:
          HStack(spacing: Spacing.l) {
            StatusDot(tone: .ready)
            StatusDot(tone: .active, pulsing: true)
            StatusDot(tone: .attention, size: .small)
            StatusDot(tone: .active, pulsing: true, size: .small)
            StatusDot(tone: .ready, size: .small)
          }
        case .pageDots:
          PageDots(index: 0, count: 6)
          PageDots(index: 2, count: 6)
          PageDots(index: 2, count: 3)
        case .progressPill:
          ProgressPill(text: String("Waiting for Accessibility…"))
          ProgressPill(text: String("Accessibility is on"), done: true)
        case .statusBadge:
          StatusBadge(label: String("Ready"), tone: .ready)
          StatusBadge(label: String("Agent working"), tone: .active)
          StatusBadge(label: String("Needs recovery"), tone: .attention)
          StatusBadge(label: String("Unavailable"), tone: .quiet)
        }
      }
      .sampleSurface()
    }
  }
#endif
