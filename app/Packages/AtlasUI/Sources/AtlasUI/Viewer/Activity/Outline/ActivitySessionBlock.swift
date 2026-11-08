import SwiftUI

/// A session as one piece: its row, then its runs one level in while it is open, or one box of its row and its newest
/// pictures while it is folded. With one session chosen its row does not show and its runs stand at the first level.
/// Its row and runs stand in the outline's stack themselves, each a place the list holds, so a run read in a long
/// session stays put as runs arrive above it. A box stands 4 from whatever is next to it. Opening it is one move: its
/// pictures fade out as its runs fade and slide in on the fold's spring, pushing the rows below down at once; folding
/// is the same move back.
struct ActivitySessionBlock: View {
  /// The session.
  let session: ViewerOutlineSession
  /// Whether the person just asked for its day's other sessions, which this one is among.
  var revealing = false
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// Whether it was open as last drawn, so the update that opens it tells its runs to come in with it.
  @State private var drawnOpen: Bool
  /// Whether its row showed as last drawn, so the update that chooses it alone, its runs standing in for it, tells
  /// them to come in with it.
  @State private var drawnHead: Bool
  @Environment(\.motionReduced) private var reduceMotion
  @Environment(\.listIsAtTop) private var isAtTop

  /// Creates the block.
  ///
  /// - Parameters:
  ///   - session: The session.
  ///   - revealing: Whether the person just asked for its day's other sessions.
  ///   - send: Receives what the person does.
  init(session: ViewerOutlineSession, revealing: Bool = false, send: @escaping @MainActor (ViewerEvent) -> Void) {
    self.session = session
    self.revealing = revealing
    self.send = send
    _drawnOpen = State(initialValue: session.isOpen)
    _drawnHead = State(initialValue: session.showsHead)
  }

  /// The row and what shows under it, each its own row of the outline.
  var body: some View {
    Group {
      if session.showsHead {
        // The row stands first whether boxed or not, so it keeps its focus and its chevron's turn as the session opens.
        VStack(alignment: .leading, spacing: .zero) {
          ActivitySessionRow(session: session, send: send)
          if session.isBoxed, let strip = session.strip {
            ActivityPictureStrip(strip: strip, send: send)
              .padding(.top, ViewerLayout.boxGap)
              .padding(.leading, ViewerLayout.outlineText(level: 0))
              .transition(ActivityMotion.disclosure(reduceMotion: reduceMotion))
          }
        }
        .outlineBox(session.isBoxed)
        .id("session.\(session.id)")
        .listPlace("session.\(session.id)")
        .transition(arrival)
      }
      if session.isOpen || !session.showsHead {
        ForEach(Array(session.runs.enumerated()), id: \.element.id) { index, run in
          ActivityRunBlock(run: run, level: session.showsHead ? 1 : 0, send: send)
            .padding(.top, gap(above: index))
            .id("run.\(run.id)")
            .listPlace("run.\(run.id)")
            .transition(runArrival)
        }
      }
    }
    .onChange(of: session.isOpen) { _, open in drawnOpen = open }
    .onChange(of: session.showsHead) { _, head in drawnHead = head }
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

  /// How its row comes in and goes: with the day's other sessions as the person asks for them, else as any row does.
  private var arrival: AnyTransition {
    guard !revealing else { return ActivityMotion.disclosure(reduceMotion: reduceMotion) }
    return ActivityMotion.row(isAtTop: isAtTop, reduceMotion: reduceMotion)
  }

  /// How its runs come in and go: with the session as the person opens it, which this update does while it was drawn
  /// folded, chooses it alone, or asks for its day's other sessions; else as any row arrives.
  private var runArrival: AnyTransition {
    let disclosing = revealing || session.isOpen != drawnOpen || session.showsHead != drawnHead
    return ActivityMotion.run(disclosing: disclosing, isAtTop: isAtTop, reduceMotion: reduceMotion)
  }
}
