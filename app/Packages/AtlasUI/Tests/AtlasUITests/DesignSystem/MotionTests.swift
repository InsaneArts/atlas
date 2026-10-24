import SwiftUI
import Testing

@testable import AtlasUI

@Suite("Motion")
struct MotionTests {
  /// The springs as the token table writes them: duration and bounce.
  private static let springs: [(name: String, spring: Spring, duration: Double, bounce: Double)] = [
    ("release", Motion.Springs.release, 0.3, 0),
    ("state", Motion.Springs.state, 0.3, 0),
    ("arrive", Motion.Springs.arrive, 0.25, 0),
    ("pop", Motion.Springs.pop, 0.35, 0.15),
    ("settle", Motion.Springs.settle, 0.5, 0.2),
    ("tab", Motion.Springs.tab, 0.25, 0),
    ("slide", Motion.Springs.slide, 0.35, 0),
    ("resize", Motion.Springs.resize, 0.25, 0),
    ("zoom", Motion.Springs.zoom, 0.35, 0),
    ("indicator", Motion.Springs.indicator, 0.3, 0),
  ]

  /// How close a spring's duration or bounce must be to its documented value.
  private static let tolerance = 1e-6

  @Test("every spring has its documented duration and bounce")
  func springsHaveTheirDurationAndBounce() {
    for (name, spring, duration, bounce) in Self.springs {
      #expect(abs(spring.duration - duration) < Self.tolerance, "\(name)")
      #expect(abs(spring.bounce - bounce) < Self.tolerance, "\(name)")
    }
  }

  @Test("a layer spring reproduces the duration and bounce of the spring it is made from")
  func layerSpringsKeepTheirFeeling() {
    for (name, _, duration, bounce) in Self.springs {
      let layer = Motion.LayerSpring(duration: duration, bounce: bounce)
      // Undamped, the mass swings once in 2π √(m / k); the damping ratio c / (2 √(k m)) is 1 − bounce.
      let swing = 2 * .pi * sqrt(layer.mass / layer.stiffness)
      let ratio = layer.damping / (2 * (layer.stiffness * layer.mass).squareRoot())
      #expect(abs(swing - duration) < Self.tolerance, "\(name)")
      #expect(abs((1 - ratio) - bounce) < Self.tolerance, "\(name)")
      // SwiftUI reads the same numbers back as the same spring.
      let read = Spring(mass: layer.mass, stiffness: layer.stiffness, damping: layer.damping)
      #expect(abs(read.duration - duration) < Self.tolerance, "\(name)")
      #expect(abs(read.bounce - bounce) < Self.tolerance, "\(name)")
    }
    let pop = Motion.LayerSpring(duration: 0.35, bounce: 0.15)
    #expect(abs(Motion.LayerSpring.pop.stiffness - pop.stiffness) < Self.tolerance)
    #expect(abs(Motion.LayerSpring.pop.damping - pop.damping) < Self.tolerance)
  }

  @Test("the slide and the indicator run on their own springs")
  func slideAndIndicatorRunOnTheirSprings() {
    #expect(Motion.slide == Animation.spring(Motion.Springs.slide))
    #expect(Motion.indicator == Animation.spring(Motion.Springs.indicator))
  }

  @Test("only a mark popping in and a success tile settling bounce; nothing else overshoots")
  func onlyTwoSpringsBounce() {
    let bouncing = Self.springs
      .filter { $0.bounce > 0 }
      .map(\.name)
    #expect(bouncing == ["pop", "settle"])
  }

  @Test("panes cross over in 0.15 seconds in and 0.1 out, at once, and words swap by opacity in 0.15")
  func panesAndWordsCrossfadeQuickly() {
    #expect(Motion.swapIn == Animation.easeOut(duration: 0.15))
    #expect(Motion.swapOut == Animation.easeIn(duration: 0.1))
    #expect(Motion.pageIn == Animation.easeOut(duration: 0.18))
    #expect(Motion.pageOut == Animation.easeIn(duration: 0.12))
    #expect(Motion.textSwap == Animation.easeOut(duration: 0.15))
    #expect(Motion.Duration.resize.seconds == 0.25)
    #expect(Motion.Duration.leastWork.seconds == 0.6)
  }

  @Test("a working note waits 0.3 seconds before it fades in, sooner than a spinner's 0.4")
  func aWorkingNoteWaitsLessThanASpinner() {
    #expect(Motion.noteAppear == Animation.easeOut(duration: 0.2).delay(0.3))
    #expect(Motion.Duration.noteDelay.seconds < Motion.Duration.appearDelay.seconds)
  }

  @Test("a press lands at once and its fill eases back over 0.12 seconds")
  func aPressLandsAtOnce() {
    #expect(CapsuleButtonStyle.pressAnimation(isPressed: true) == nil)
    #expect(CapsuleButtonStyle.pressAnimation(isPressed: false) == Animation.easeOut(duration: 0.12))
  }

  @Test("the tab thumb glides with no overshoot, and jumps at once under Reduce Motion")
  func theThumbGlides() {
    #expect(TabsLayout.thumbGlide(reduceMotion: false) == Motion.tab)
    #expect(TabsLayout.thumbGlide(reduceMotion: true) == nil)
  }

  @Test("a reduced animation is a fade of at most 0.2 seconds")
  func reducedIsAShortFade() {
    #expect(Motion.Duration.reduced.seconds <= 0.2)
    #expect(Motion.reduced == Animation.easeOut(duration: Motion.Duration.reduced.seconds))
    #expect(Motion.animation(Motion.arrive, reduceMotion: true, reduced: Motion.reduced) == Motion.reduced)
    #expect(Motion.animation(Motion.arrive, reduceMotion: false, reduced: Motion.reduced) == Motion.arrive)
  }

  @Test("a loop runs only without Reduce Motion")
  func loopsStopUnderReduceMotion() {
    #expect(Motion.loops(reduceMotion: false))
    #expect(!Motion.loops(reduceMotion: true))
  }

  @Test("a hover comes in faster than it goes out")
  func hoverInIsFasterThanOut() {
    #expect(Motion.hover(true) == Animation.easeOut(duration: Motion.Duration.hoverIn.seconds))
    #expect(Motion.hover(false) == Animation.easeOut(duration: Motion.Duration.hoverOut.seconds))
    #expect(Motion.Duration.hoverIn.seconds < Motion.Duration.hoverOut.seconds)
  }
}
