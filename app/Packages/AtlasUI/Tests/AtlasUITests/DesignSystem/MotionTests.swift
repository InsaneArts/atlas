import SwiftUI
import Testing

@testable import AtlasUI

@Suite("Motion")
struct MotionTests {
  /// v3's springs (D14) as the token table writes them: duration and bounce.
  private static let springs: [(name: String, spring: Spring, duration: Double, bounce: Double)] = [
    ("release", Motion.Springs.release, 0.3, 0.2),
    ("state", Motion.Springs.state, 0.3, 0),
    ("arrive", Motion.Springs.arrive, 0.3, 0.15),
    ("page", Motion.Springs.page, 0.42, 0),
    ("pop", Motion.Springs.pop, 0.35, 0.3),
    ("tab", Motion.Springs.tab, 0.32, 0.12),
    ("slide", Motion.Springs.slide, 0.35, 0),
    ("resize", Motion.Springs.resize, 0.35, 0),
    ("zoom", Motion.Springs.zoom, 0.4, 0.12),
    ("indicator", Motion.Springs.indicator, 0.45, 0.2),
  ]

  /// How close a spring's duration or bounce must be to its documented value.
  private static let tolerance = 1e-6

  @Test("every v3 spring has its documented duration and bounce")
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
    #expect(Motion.LayerSpring.pop == Motion.LayerSpring(duration: 0.35, bounce: 0.3))
  }

  @Test("the slide and indicator are still today's springs, exactly")
  func slideAndIndicatorKeepTheirSprings() {
    #expect(Motion.slide == Animation.spring(response: 0.35, dampingFraction: 1))
    #expect(Motion.indicator == Animation.spring(response: 0.45, dampingFraction: 0.8))
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
