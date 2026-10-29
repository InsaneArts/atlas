#if DEBUG
  import SwiftUI

  /// The empty message's samples: connecting with no button, logged out with a secondary one, and not ready with
  /// the next step, the last in a small window; then the main window's stage pattern, its mark ringed while
  /// connecting, and a pane's, its symbol in a tile.
  struct EmptyMessageSamples: View {
    /// The messages, one above the other.
    var body: some View {
      VStack(spacing: Spacing.m) {
        EmptyMessage(
          state: EmptyMessageState(
            headline: "Connecting to the helper…",
            message: "This takes a moment after the Mac starts."
          )
        )
        EmptyMessage(
          state: EmptyMessageState(
            headline: "The helper is logged out",
            message: "Agents can't use it until you log in to it again.",
            action: EmptyMessageAction(title: "Switch to the Helper…", isPrimary: false)
          )
        )
        EmptyMessage(
          state: EmptyMessageState(
            headline: "Atlas isn't answering in the helper",
            message: "Recover checks the helper and starts it again.",
            action: EmptyMessageAction(title: "Recover…", isPrimary: true)
          ),
          size: .small
        )
        EmptyMessage(
          state: EmptyMessageState(
            headline: "Connecting to the helper…",
            message: "This takes a moment after the Mac starts."
          ),
          size: .stage,
          tracesMark: true
        )
        EmptyMessage(
          state: EmptyMessageState(
            headline: "No screenshots yet",
            message: "Screenshots agents take in the helper show up here."
          ),
          size: .stage,
          symbol: .photos
        )
      }
      .frame(width: SampleWidth.column, height: SampleWidth.column * 5 / 2)
      .sampleSurface()
    }
  }
#endif
