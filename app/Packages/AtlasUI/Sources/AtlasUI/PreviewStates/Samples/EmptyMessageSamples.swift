#if DEBUG
  import SwiftUI

  /// The empty message's samples: connecting with no button, logged out with a glass one, and not ready with the
  /// next step, the last in a small window.
  struct EmptyMessageSamples: View {
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
      }
      .frame(width: SampleWidth.column, height: SampleWidth.column * 3 / 2)
      .sampleSurface()
    }
  }
#endif
