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
      /// Title and detail pairs at each size.
      case titleDetail
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
        case .titleDetail:
          TitleDetail(title: String("Claude Code"), detail: String("Found. Adds the Atlas skill to every session."))
          TitleDetail(
            title: String("The helper is logged out"),
            detail: String("Log in to it to use it again."),
            size: .compact,
            wraps: true
          )
        }
      }
      .frame(width: SampleWidth.column)
      .sampleSurface()
    }
  }
#endif
