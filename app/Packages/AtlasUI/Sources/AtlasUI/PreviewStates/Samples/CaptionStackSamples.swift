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
      ActivityEntry(id: "1", line: "Scrolled in Opie", when: "5 s ago", isNewest: false),
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

    /// The actions an agent has taken after `count` steps, oldest first, the newest marked: what the Interactive
    /// preview pushes in one at a time.
    ///
    /// - Parameter count: How many actions so far.
    /// - Returns: The actions.
    static func entries(count: Int) -> [ActivityEntry] {
      (0..<count)
        .map { step in
          ActivityEntry(
            id: String(step),
            line: step.isMultiple(of: 2) ? "Clicked Ring in Opie" : "Scrolled in Opie",
            when: step == count - 1 ? "just now" : "\((count - 1 - step) * 4) s ago",
            isNewest: step == count - 1
          )
        }
    }

    /// The two piles side by side.
    var body: some View {
      HStack(alignment: .bottom, spacing: Spacing.xxl) {
        CaptionStack(entries: Self.working)
        CaptionStack(entries: Self.failed)
      }
      .sampleSurface()
    }
  }
#endif
