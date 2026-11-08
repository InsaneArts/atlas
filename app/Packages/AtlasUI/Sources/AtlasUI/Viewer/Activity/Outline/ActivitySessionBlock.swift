import SwiftUI

/// A session as one piece: its row, then its runs one level in while it is open, or one box of its row and its newest
/// pictures while it is folded. With one session chosen its row does not show and its runs stand at the first level.
/// Its row and runs stand in the outline's stack themselves, each a place the list holds, so a run read in a long
/// session stays put as runs arrive above it. A box stands 4 from whatever is next to it.
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
      // The row stands first whether boxed or not, so it keeps its focus and its chevron's turn as the session opens.
      VStack(alignment: .leading, spacing: .zero) {
        ActivitySessionRow(session: session, send: send)
        if session.isBoxed, let strip = session.strip {
          ActivityPictureStrip(strip: strip, send: send)
            .padding(.top, ViewerLayout.boxGap)
            .padding(.leading, ViewerLayout.outlineText(level: 0))
            .transition(.opacity)
        }
      }
      .outlineBox(session.isBoxed)
      .id("session.\(session.id)")
      .transition(arrival)
    }
    if session.isOpen || !session.showsHead {
      ForEach(Array(session.runs.enumerated()), id: \.element.id) { index, run in
        ActivityRunBlock(run: run, level: session.showsHead ? 1 : 0, send: send)
          .padding(.top, gap(above: index))
          .id("run.\(run.id)")
          .transition(arrival)
      }
    }
  }

  /// The room above a run: 4 when it or what stands above it, the session's row or the run before, is a box; none
  /// for the first run with no row above it.
  ///
  /// - Parameter index: The run's place.
  /// - Returns: The room.
  private func gap(above index: Int) -> CGFloat {
    let boxed = session.runs[index].isBoxed || (index > 0 && session.runs[index - 1].isBoxed)
    let anything = index > 0 || session.showsHead
    return boxed && anything ? ViewerLayout.boxGap : .zero
  }

  /// How a row comes in and goes.
  private var arrival: AnyTransition {
    ActivityMotion.row(isAtTop: isAtTop, reduceMotion: reduceMotion)
  }
}
