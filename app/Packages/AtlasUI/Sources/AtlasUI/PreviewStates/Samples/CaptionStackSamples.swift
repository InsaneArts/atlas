#if DEBUG
  import SwiftUI

  /// The caption stack's samples: an agent's actions, oldest first, then the same with the newest failed.
  struct CaptionStackSamples: View {
    /// Two older actions and the newest.
    private static let working = [
      ActivityEntry(
        id: "2",
        line: "Clicked Ring in Opie",
        when: "9 s ago",
        isNewest: false
      ),
      ActivityEntry(id: "1", line: "Scrolled in Opie", when: "4 s ago", isNewest: false),
      ActivityEntry(
        id: "0",
        line: "Turning Show Center off in Opie",
        when: "just now",
        isNewest: true
      ),
    ]

    /// The same, with the newest action failed.
    private static let failed = [
      working[0],
      working[1],
      ActivityEntry(
        id: "0",
        line: "Couldn't click Show Centre in Opie",
        when: "just now",
        mark: .failed,
        isNewest: true
      ),
    ]

    var body: some View {
      HStack(alignment: .bottom, spacing: Spacing.xxl) {
        CaptionStack(entries: Self.working)
        CaptionStack(entries: Self.failed)
      }
      .sampleSurface()
    }
  }
#endif
