#if DEBUG
  import SwiftUI

  /// The Activity list's pieces on their own: steps of every kind and mark, moments open, folded and held, and
  /// Interactive ones where the newest step arrives again or a moment folds and opens.
  struct ActivitySamples: View {
    /// Which samples show.
    enum Kind: Sendable {
      /// The working sample's steps.
      case steps
      /// The newest step, arriving again on a press.
      case arriving
      /// The working sample's moments' headers.
      case moments
      /// A moment that folds and opens on a press.
      case folding
    }

    /// Which samples show.
    private let kind: Kind
    /// How many times the newest step arrived again.
    @State private var arrivals = 0
    /// Whether the folding sample's moment is open.
    @State private var isOpen = false

    /// Creates the samples.
    ///
    /// - Parameter kind: Which samples show.
    init(_ kind: Kind) {
      self.kind = kind
    }

    /// The samples.
    var body: some View {
      switch kind {
      case .steps:
        column(steps.map { ActivityStepRow(step: $0) { _ in } })
          .sampleSurface()
      case .arriving:
        InteractivePreview {
          Button(String("Arrive again")) { arrivals += 1 }
        } content: {
          if let step = steps.first {
            ActivityStepRow(step: step) { _ in }
              .id(arrivals)
              .transition(ActivityMotion.row(isAtTop: true, reduceMotion: false))
              .frame(width: SampleWidth.column * 2)
          }
        }
      case .moments:
        column(moments.map { ActivityMomentRow(moment: $0) { _ in } })
          .sampleSurface()
      case .folding:
        InteractivePreview {
          Toggle(String("Open"), isOn: $isOpen)
        } content: {
          if let moment = moments.last {
            ActivityMomentRow(moment: shown(moment)) { _ in isOpen.toggle() }
              .frame(width: SampleWidth.column * 2)
          }
        }
      }
    }

    /// The folding sample's moment, open or folded as the toggle says.
    ///
    /// - Parameter moment: The moment.
    /// - Returns: It, opened or folded.
    private func shown(_ moment: ViewerActivityMoment) -> ViewerActivityMoment {
      var shown = moment
      shown.isOpen = isOpen
      shown.canFold = true
      return shown
    }

    /// Rows one under another, two columns wide.
    ///
    /// - Parameter rows: The rows.
    /// - Returns: The column.
    private func column(_ rows: [some View]) -> some View {
      VStack(alignment: .leading, spacing: .zero) {
        ForEach(rows.indices, id: \.self) { index in
          rows[index]
        }
      }
      .frame(width: SampleWidth.column * 2)
    }

    /// The working sample's steps, and the problems sample's.
    private var steps: [ViewerActivityStep] {
      (PreviewStates.workingActivity.days + PreviewStates.problemsActivity.days)
        .flatMap(\.moments)
        .flatMap(\.steps)
    }

    /// The working sample's moments, and the session sample's.
    private var moments: [ViewerActivityMoment] {
      (PreviewStates.workingActivity.days + PreviewStates.bySessionActivity.days)
        .flatMap(\.moments)
    }
  }
#endif
