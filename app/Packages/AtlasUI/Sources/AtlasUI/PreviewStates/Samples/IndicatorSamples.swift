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
      /// Every mark, a breathing dot and the small dots a caption and a badge use.
      case statusDot
      /// The first, a middle and the last step.
      case pageDots
      /// Waiting and done.
      case progressPill
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
        case .checkMark:
          HStack(spacing: Spacing.m) {
            CheckMark()
            CheckMark(size: IconSize.m)
            CheckMark(size: IconSize.xxl)
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
            Disc(content: .letter(String("I")), tone: .avatar, size: .large)
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
        case .pageDots:
          PageDots(index: 0, count: 6)
          PageDots(index: 2, count: 6)
          PageDots(index: 2, count: 3)
        case .progressPill:
          ProgressPill(text: String("Waiting for Accessibility…"))
          ProgressPill(text: String("Accessibility is on"), done: true)
        }
      }
      .sampleSurface()
    }
  }
#endif
