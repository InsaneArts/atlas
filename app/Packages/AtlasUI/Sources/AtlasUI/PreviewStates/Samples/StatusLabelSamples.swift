#if DEBUG
  import SwiftUI

  /// The status label's samples: each row of the status table that has a line, then a line too long for its room.
  struct StatusLabelSamples: View {
    /// One status per row of the table.
    private static let statuses = [
      StatusSummary(label: "Working", tone: .active, mark: .pulsing, line: "Turning Show Center off in Opie"),
      StatusSummary(label: "Idle", tone: .quiet, mark: .dot, line: "Last: turned Show Center off · 2 min ago"),
      StatusSummary(label: "You have control", tone: .link, mark: .dot, line: "The agent waits"),
      StatusSummary(label: "Needs you", tone: .attention, mark: .dot, line: "An action may not have finished"),
      StatusSummary(label: "Stopped", tone: .attention, mark: .square, line: "The agent waits for you"),
      StatusSummary(label: "Not ready", tone: .quiet, mark: .small),
    ]

    var body: some View {
      VStack(alignment: .leading, spacing: Spacing.m) {
        ForEach(Self.statuses, id: \.label) { status in
          StatusLabel(status: status)
        }
        StatusLabel(status: Self.statuses[0])
          .frame(width: SampleWidth.narrow, alignment: .leading)
      }
      .sampleSurface()
    }
  }
#endif
