import SwiftUI

/// The top of every setup screen, in the window's one rhythm (v3 D2): a hero if the screen has one, a label, the
/// title, and the paragraph under it. The hero, the label and the title arrive as the screen's header, the paragraph
/// with its content (v3 S2).
struct OnboardingHeader<Hero: View>: View {
  /// The icon, pairing or mark above the title, if any.
  private let hero: Hero?
  /// The small label over the title, if any.
  private let label: String?
  /// The title's statement.
  private let title: String
  /// The title's quieter second line, if any.
  private let tail: String?
  /// The paragraph under the title, if any.
  private let lede: String?

  /// Creates a screen's header with a hero.
  ///
  /// - Parameters:
  ///   - label: The small label over the title, if any.
  ///   - title: The title's statement.
  ///   - tail: The title's quieter second line, if any.
  ///   - lede: The paragraph under the title, if any.
  ///   - hero: The icon, pairing or mark above the title.
  init(
    label: String? = nil,
    title: String,
    tail: String? = nil,
    lede: String? = nil,
    @ViewBuilder hero: () -> Hero
  ) {
    self.hero = hero()
    self.label = label
    self.title = title
    self.tail = tail
    self.lede = lede
  }

  /// The hero, the label and the title, then the paragraph.
  var body: some View {
    VStack(spacing: .zero) {
      VStack(spacing: .zero) {
        if let hero {
          hero
            .padding(.bottom, OnboardingLayout.Rhythm.hero)
        }
        if let label {
          GroupLabel(text: label)
            .lineLimit(OnboardingLayout.nameLines)
            .truncationMode(.middle)
            .multilineTextAlignment(.center)
            .padding(.bottom, OnboardingLayout.Rhythm.label)
        }
        Headline(title: title, tail: tail)
      }
      .pagePiece(.header)
      if let lede {
        Lede(text: lede)
          .padding(.top, OnboardingLayout.Rhythm.title)
          .pagePiece(.content)
      }
    }
  }
}

extension OnboardingHeader where Hero == EmptyView {
  /// Creates a screen's header with no hero.
  ///
  /// - Parameters:
  ///   - label: The small label over the title, if any.
  ///   - title: The title's statement.
  ///   - tail: The title's quieter second line, if any.
  ///   - lede: The paragraph under the title, if any.
  init(label: String? = nil, title: String, tail: String? = nil, lede: String? = nil) {
    self.hero = nil
    self.label = label
    self.title = title
    self.tail = tail
    self.lede = lede
  }
}

#if DEBUG
  #Preview("Label, title and paragraph") {
    OnboardingHeader(
      label: String("Helper account"),
      title: String("Agents need an account"),
      tail: String("of their own."),
      lede: String("A standard account on this Mac that only Atlas uses.")
    )
    .frame(width: SampleWidth.column)
    .sampleSurface()
  }

  #Preview("A hero and a long name") {
    OnboardingHeader(
      label: String("Atlas in Maria Fernanda Montgomery-Castellanos de la Vega"),
      title: String("Let Christopher Alexander Montgomery-Smith use this account"),
      tail: String("through Atlas?")
    ) {
      AppIcon(size: OnboardingLayout.allowIcon)
    }
    .frame(width: SampleWidth.column)
    .sampleSurface()
  }
#endif
