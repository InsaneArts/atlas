#if DEBUG
  import SwiftUI

  /// The text components' samples, one kind at a time.
  struct TextSamples: View {
    /// Which component the samples show.
    enum Kind: Sendable {
      /// Headlines at each size.
      case headline
      /// A lede.
      case lede
      /// Notes in both tones.
      case note
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
      VStack(spacing: Spacing.mPlus) {
        switch kind {
        case .headline:
          Headline(title: String("Give your agents"), tail: String("a Mac of their own."))
          Headline(
            title: String("Now log in to Maria Fernanda Montgomery-Castellanos de la Vega y Rodríguez."),
            tail: String("Just this once, to set Atlas up there.")
          )
          .frame(width: SampleWidth.column)
          Headline(title: String("The helper is"), tail: String("logged out."), size: .title)
          Headline(title: String("Turn on Atlas."), size: .compact)
        case .lede:
          Lede(
            text: String(
              "Atlas runs a hidden second login on this Mac. Your agents open their builds there while you keep working."
            )
          )
        case .note:
          Note(text: String("You set this up once, in about five minutes."))
          Note(text: String("The passwords don't match."), tone: .attention)
        }
      }
      .frame(width: SampleWidth.column)
      .sampleSurface()
    }
  }
#endif
