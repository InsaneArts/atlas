#if DEBUG
  import SwiftUI

  /// The status line's samples: each kind with a representative status sentence, and its marks alone.
  struct StatusLineSamples: View {
    /// Which samples to show.
    enum Kind: Sendable {
      /// Working, a problem and done, each with its corresponding mark.
      case kinds
      /// The three marks.
      case marks
    }

    private let kind: Kind

    /// Creates the samples of one kind.
    ///
    /// - Parameter kind: Which samples to show.
    init(_ kind: Kind) {
      self.kind = kind
    }

    /// A representative status sentence for a kind.
    ///
    /// - Parameter kind: How things stand.
    /// - Returns: The status sentence shown for it.
    static func sentence(_ kind: StatusLine.Kind) -> String {
      switch kind {
      case .working: String("Next: Lock and Switch, then log in to the Atlas account.")
      case .problem: String("macOS could not say whether kid is an administrator.")
      case .done: String("Atlas is up to date.")
      }
    }

    /// The samples, on the ground.
    var body: some View {
      VStack(spacing: Layout.lineGap) {
        switch kind {
        case .kinds:
          StatusLine(kind: .working, text: Self.sentence(.working))
          StatusLine(kind: .problem, text: Self.sentence(.problem))
          StatusLine(kind: .done, text: Self.sentence(.done))
        case .marks:
          HStack(spacing: Spacing.m) {
            OutcomeMark(kind: .working)
            OutcomeMark(kind: .problem)
            OutcomeMark(kind: .done)
          }
        }
      }
      .sampleSurface()
    }
  }

  /// Spacing that separates status samples for comparison.
  private enum Layout {
    /// 18: between two status lines, a 4-point gap and each line's 14 above it.
    static let lineGap: CGFloat = 18
  }
#endif
