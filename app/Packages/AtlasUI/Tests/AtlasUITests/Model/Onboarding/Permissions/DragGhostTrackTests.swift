import Testing

@testable import AtlasUI

@Suite("DragGhostTrack")
struct DragGhostTrackTests {
  @Test("the ghost appears on the icon, travels to the list, fades as it lands, then rests unseen")
  func oneLoop() {
    #expect(DragGhostTrack.at(0) == DragGhostTrack(progress: 0, opacity: 0, scale: 1))
    #expect(DragGhostTrack.at(DragGhostTrack.appear).opacity == 1)
    let halfway = DragGhostTrack.at((DragGhostTrack.appear + DragGhostTrack.travel) / 2)
    #expect(abs(halfway.progress - 0.5) < 0.001 && halfway.opacity == 1)
    let landed = DragGhostTrack.at(DragGhostTrack.land)
    #expect(landed.progress == 1 && landed.opacity == 0 && landed.scale < 1)
    #expect(DragGhostTrack.at(DragGhostTrack.cycle - 0.01).opacity == 0)
  }

  @Test("the loop repeats every cycle, on any clock")
  func repeats() {
    let moment = 0.9
    let base = DragGhostTrack.at(moment)
    for later in [moment + DragGhostTrack.cycle * 1000, moment - DragGhostTrack.cycle] {
      let track = DragGhostTrack.at(later)
      #expect(abs(track.progress - base.progress) < 0.000_1 && abs(track.opacity - base.opacity) < 0.000_1)
    }
  }

  @Test("the path eases out of the icon and into the list, always moving forward")
  func eases() {
    let steps = stride(from: 0.0, through: 1.0, by: 0.05)
      .map(DragGhostTrack.eased)
    #expect(steps.first == 0 && steps.last == 1)
    #expect(zip(steps, steps.dropFirst()).allSatisfy { $0 <= $1 })
    #expect(DragGhostTrack.eased(0.1) < 0.1 && DragGhostTrack.eased(0.9) > 0.9)
  }
}
