#if DEBUG
  import SwiftUI

  /// The Activity outline's pieces on their own: steps of every kind and mark, runs folded and open, and Interactive
  /// ones where the newest step arrives again or a run folds and opens.
  struct ActivitySamples: View {
    /// Which samples show.
    enum Kind: Sendable {
      /// The samples' steps.
      case steps
      /// The newest step, arriving again on a press.
      case arriving
      /// The samples' runs.
      case runs
      /// A run that folds and opens on a press.
      case folding
    }

    /// Which samples show.
    private let kind: Kind
    /// How many times the newest step arrived again.
    @State private var arrivals = 0
    /// Whether the folding sample's run is open.
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
        column(PreviewStates.chromeSteps.map { ActivityStepRow(step: $0, level: 0) { _ in } })
          .sampleSurface()
      case .arriving:
        InteractivePreview {
          Button(String("Arrive again")) { arrivals += 1 }
        } content: {
          if let step = PreviewStates.chromeSteps.first {
            ActivityStepRow(step: step, level: 0) { _ in }
              .id(arrivals)
              .transition(ActivityMotion.row(isAtTop: true, reduceMotion: false))
              .frame(width: SampleWidth.column * 2)
          }
        }
      case .runs:
        column(PreviewStates.claudeSession.runs.map { ActivityRunBlock(run: $0, level: 0) { _ in } })
          .sampleSurface()
      case .folding:
        InteractivePreview {
          Toggle(String("Open"), isOn: $isOpen)
        } content: {
          ActivityRunBlock(run: isOpen ? PreviewStates.chromeRunOpen : PreviewStates.chromeRun, level: 0) { _ in
            isOpen.toggle()
          }
          .frame(width: SampleWidth.column * 2)
        }
      }
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
  }
#endif
