import CoreGraphics
import Testing

@testable import AtlasUI

@Suite("DragGhostView")
struct DragGhostViewTests {
  @Test("Reduce Motion stills the ghost and keeps its hint at the target")
  func reduceMotionStillsTheGhost() {
    let state = DragGhostState(from: CGPoint(x: 600, y: 300), to: CGPoint(x: 200, y: 120), start: .now)
    let moments = stride(from: 0.0, to: DragGhostTrack.cycle * 2, by: 0.25)
    for moment in moments {
      let track = DragGhostView.track(at: moment, reduceMotion: true)
      #expect(track == .landed, "\(moment) s in, it rests")
      #expect(state.point(at: track.progress) == state.to, "at the list")
      #expect(track.opacity > 0, "and shows")
    }
    // With motion, it flies: at the icon as it appears, part way across mid-flight.
    #expect(DragGhostView.track(at: 0, reduceMotion: false).progress == 0)
    let midway = DragGhostView.track(at: (DragGhostTrack.appear + DragGhostTrack.travel) / 2, reduceMotion: false)
    #expect(midway.progress > 0 && midway.progress < 1)
  }
}
