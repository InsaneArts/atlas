import SwiftUI

/// A session as one piece: its row, then its runs one level in while it is open, or its newest pictures while it is
/// folded. With one session chosen its row does not show and its runs stand at the first level. Its row and runs stand
/// in the outline's stack themselves, each a place the list holds, so a run read in a long session stays put as runs
/// arrive above it.
struct ActivitySessionBlock: View {
  /// The session.
  let session: ViewerOutlineSession
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.motionReduced) private var reduceMotion
  @Environment(\.listIsAtTop) private var isAtTop

  /// The row and what shows under it, each its own row of the outline.
  var body: some View {
    if session.showsHead {
      ActivitySessionRow(session: session, send: send)
        .id("session.\(session.id)")
        .transition(arrival)
    }
    if session.isOpen || !session.showsHead {
      ForEach(session.runs) { run in
        ActivityRunBlock(run: run, level: session.showsHead ? 1 : 0, send: send)
          .id("run.\(run.id)")
          .transition(arrival)
      }
    } else if let strip = session.strip {
      ActivityPictureStrip(strip: strip, send: send)
        .padding(.leading, ViewerLayout.outlineText(level: 0))
        .id("strip.\(session.id)")
        .transition(.opacity)
    }
  }

  /// How a row comes in and goes.
  private var arrival: AnyTransition {
    ActivityMotion.row(isAtTop: isAtTop, reduceMotion: reduceMotion)
  }
}
