import Foundation
import Testing

@testable import AtlasUI

@Suite("Segmented control motion")
struct TabsMotionTests {
  @Test("a drag releases into the nearest segment centre rather than projecting into another choice")
  func aDragReleasesIntoTheNearestSegmentCentre() {
    var motion = Self.motion()
    motion.press(at: 20, time: 0)
    motion.drag(to: 110, reduced: false)
    #expect(motion.geometry.segment(at: motion.centre) == 1)
    #expect(motion.selected == 0)
    #expect(motion.release(at: 110, velocity: 900, time: 0.1, reduced: false) == 1)
    #expect(motion.spring?.target == 92)
    #expect(motion.geometry.nearest(to: 56) == 0)
    #expect(motion.geometry.nearest(to: 56.1) == 1)
  }

  @Test("a tap chooses the segment containing the pointer even beside a wider segment's leading edge")
  func aTapChoosesTheSegmentContainingThePointer() {
    var motion = Self.motion()
    motion.press(at: 145, time: 0)
    #expect(motion.geometry.nearest(to: 145) == 1)
    #expect(motion.release(at: 145, velocity: 0, time: 0.1, reduced: false) == 2)
  }

  @Test("a release carries the drag velocity continuously into its critically damped spring")
  func aReleaseCarriesTheDragVelocity() {
    var motion = Self.motion()
    motion.press(at: 20, time: 0)
    motion.drag(to: 100, reduced: false)
    let before = motion.centre
    _ = motion.release(at: 100, velocity: 180, time: 0.1, reduced: false)
    #expect(motion.sample(at: 0.1).position == before)
    #expect(motion.sample(at: 0.1).velocity == 180)
    #expect(motion.sample(at: 0.1001).position > before)
    #expect(motion.sample(at: 2).position == 92)
    #expect(motion.sample(at: 2).velocity == 0)
  }

  @Test("the thumb interpolates unequal widths continuously between the segment centres")
  func theThumbInterpolatesUnequalWidths() {
    let geometry = Self.motion().geometry
    #expect(geometry.thumb(at: 20).width == 40)
    #expect(geometry.thumb(at: 56).width == 70)
    #expect(geometry.thumb(at: 92).width == 100)
    #expect(geometry.thumb(at: 158).width == 130)
    #expect(geometry.thumb(at: 224).width == 160)
    #expect(geometry.thumb(at: -10).width == 40)
    #expect(geometry.thumb(at: 400).width == 160)
    for centre in stride(from: 20.0, through: 224.0, by: 0.5) {
      let frame = geometry.thumb(at: centre)
      #expect(abs(frame.midX - centre) < 0.0001)
      #expect(abs(geometry.thumb(at: centre + 0.001).width - frame.width) < 0.001)
    }
  }

  @Test("a pointer choice is sent once on release and never during preview or a repeated release")
  func aPointerChoiceIsSentOnceOnRelease() {
    var motion = Self.motion()
    var choices: [Int] = []
    motion.press(at: 20, time: 0)
    for pointer in [70.0, 150, 224, 100] {
      motion.drag(to: pointer, reduced: false)
      #expect(motion.selected == 0)
      #expect(choices.isEmpty)
    }
    if let choice = motion.release(at: 100, velocity: 0, time: 0.2, reduced: false) { choices.append(choice) }
    if let choice = motion.release(at: 100, velocity: 0, time: 0.3, reduced: false) { choices.append(choice) }
    #expect(choices == [1])
    #expect(!motion.isPressed)
  }

  @Test("a tap retargeting mid-slide preserves the thumb position and velocity at the second release")
  func aTapRetargetingMidSlidePreservesThePresentation() {
    var motion = Self.motion()
    motion.press(at: 224, time: 0)
    #expect(motion.release(at: 224, velocity: 0, time: 0, reduced: false) == 2)
    motion.press(at: 20, time: 0.09)
    motion.drag(to: 21, reduced: false)
    let live = motion.sample(at: 0.12)
    #expect(live.position > 20 && live.position < 224)
    #expect(live.velocity > 0)
    #expect(motion.release(at: 21, velocity: 0, time: 0.12, reduced: false) == 0)
    #expect(abs(motion.sample(at: 0.12).position - live.position) < 0.0001)
    #expect(abs(motion.sample(at: 0.12).velocity - live.velocity) < 0.0001)
    #expect(motion.spring?.target == 20)
  }

  @Test("a drag grabbed off centre follows the pointer one for one without moving the labels")
  func aDragKeepsItsGrabOffset() {
    var motion = Self.motion()
    let geometry = motion.geometry
    motion.press(at: 12, time: 0)
    motion.drag(to: 62, reduced: false)
    #expect(motion.centre == 70)
    #expect(motion.geometry == geometry)
    var track = Self.motion()
    track.press(at: 160, time: 0)
    track.drag(to: 180, reduced: false)
    #expect(track.centre == 180)
  }

  @Test("grabbing a moving thumb freezes its presentation before a drag follows the pointer")
  func grabbingAMovingThumbFreezesItsPresentation() {
    var motion = Self.motion()
    motion.choose(2, at: 0, reduced: false)
    let live = motion.sample(at: 0.08)
    motion.press(at: live.position - 4, time: 0.08)
    #expect(abs(motion.sample(at: 0.12).position - live.position) < 0.0001)
    #expect(abs(motion.sample(at: 0.12).velocity - live.velocity) < 0.0001)
    motion.drag(to: live.position + 16, reduced: false)
    #expect(abs(motion.centre - (live.position + 20)) < 0.0001)
  }

  @Test("rubber banding resists the ends and hands its reduced velocity to the release spring")
  func rubberBandingResistsTheEnds() {
    var motion = Self.motion()
    motion.press(at: 20, time: 0)
    motion.drag(to: -50, reduced: false)
    #expect(motion.centre < 20)
    #expect(motion.centre > 20 - TabsLayout.rubberBandLimit)
    let expected = motion.geometry.resisted(-50, reduced: false).slope * -100
    #expect(motion.release(at: -50, velocity: -100, time: 0.2, reduced: false) == 0)
    #expect(abs(motion.sample(at: 0.2).velocity - expected) < 0.0001)
  }

  @Test("reduced motion crossfades taps and clamps drags before settling them immediately")
  func reducedMotionCrossfadesAndClamps() {
    var motion = Self.motion()
    motion.press(at: 224, time: 0)
    _ = motion.release(at: 224, velocity: 0, time: 0, reduced: true)
    #expect(motion.spring == nil)
    #expect(motion.outgoing?.midX == 20)
    #expect(motion.frame(at: 0).midX == 224)
    #expect(motion.fade(at: 0) == 0)
    #expect(motion.fade(at: Motion.Duration.crossfadeReduced.seconds / 2) == 0.5)
    motion.press(at: 224, time: 0.2)
    motion.drag(to: -100, reduced: true)
    #expect(motion.centre == 20)
    _ = motion.release(at: 100, velocity: 500, time: 0.3, reduced: true)
    #expect(motion.spring == nil)
    #expect(motion.outgoing == nil)
    #expect(motion.frame(at: 0.3).midX == 92)
  }

  @Test("an external choice uses the same retargeting and an unchanged choice keeps a release velocity")
  func anExternalChoiceUsesTheSameRetargeting() {
    var motion = Self.motion()
    motion.choose(2, at: 0, reduced: false)
    let live = motion.sample(at: 0.1)
    motion.update(frames: Self.frames, selected: 1, at: 0.1, reduced: false)
    #expect(abs(motion.sample(at: 0.1).position - live.position) < 0.0001)
    #expect(abs(motion.sample(at: 0.1).velocity - live.velocity) < 0.0001)
    let spring = motion.spring
    motion.update(frames: Self.frames, selected: 1, at: 0.12, reduced: false)
    #expect(motion.spring == spring)
  }

  @Test("a cancelled press produces no choice and an empty track remains inert")
  func aCancelledPressProducesNoChoice() {
    var motion = Self.motion()
    motion.press(at: 20, time: 0)
    motion.drag(to: 100, reduced: false)
    motion.cancel()
    #expect(motion.release(at: 100, velocity: 0, time: 0.1, reduced: false) == nil)
    var empty = TabsMotion()
    empty.press(at: 20, time: 0)
    #expect(empty.release(at: 100, velocity: 0, time: 0.1, reduced: false) == nil)
  }

  /// Unequal padded segments, with the same two-point gaps as the real track.
  private static let frames = [
    CGRect(x: 0, y: 2, width: 40, height: 24),
    CGRect(x: 42, y: 2, width: 100, height: 24),
    CGRect(x: 144, y: 2, width: 160, height: 24),
  ]

  /// A measured track resting on its first segment.
  private static func motion() -> TabsMotion {
    var motion = TabsMotion()
    motion.update(frames: frames, selected: 0, at: 0, reduced: false)
    return motion
  }
}
