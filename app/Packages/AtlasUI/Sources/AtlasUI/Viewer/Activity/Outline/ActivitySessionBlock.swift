import SwiftUI

/// A session as one piece: its row, then its runs one level in while it is open, or its newest pictures while it is
/// folded. With one session chosen its row does not show and its runs stand at the first level.
struct ActivitySessionBlock: View {
  /// The session.
  let session: ViewerOutlineSession
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.motionReduced) private var reduceMotion
  @Environment(\.listIsAtTop) private var isAtTop

  /// The row and what shows under it.
  var body: some View {
    VStack(alignment: .leading, spacing: .zero) {
      if session.showsHead {
        ActivitySessionRow(session: session, send: send)
      }
      if session.isOpen || !session.showsHead {
        ForEach(session.runs) { run in
          ActivityRunBlock(run: run, level: session.showsHead ? 1 : 0, send: send)
            .id("run.\(run.id)")
            .transition(ActivityMotion.row(isAtTop: isAtTop, reduceMotion: reduceMotion))
        }
      } else if let strip = session.strip {
        ActivityPictureStrip(strip: strip, send: send)
          .padding(.leading, ViewerLayout.outlineText(level: 0))
          .transition(.opacity)
      }
    }
    .accessibilityElement(children: .contain)
  }
}
