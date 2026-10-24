import AppKit
import AtlasUI
import Testing

@testable import AtlasApp

@MainActor
@Suite("WindowGlide")
struct WindowGlideTests {
  @Test("a move between the card and the panel fades the content out, glides, and brings it back from 80%")
  func aResizingMoveFadesItsContent() {
    let steps = GlideSteps(reduceMotion: false, resizes: true)
    #expect(steps.fading == .content)
    #expect(steps.fadeOut == Motion.Duration.contentOut.seconds)
    #expect(steps.travel == Motion.Duration.glideSoft.seconds)
    #expect(steps.fadeInDelay == Motion.Duration.glideSoft.seconds * Motion.glideReveal)
    #expect(steps.fadeIn == Motion.Duration.fade.seconds)
    let same = GlideSteps(reduceMotion: false, resizes: false)
    #expect(same.fading == .none && same.travel == Motion.Duration.glideSoft.seconds, "a panel moving glides alone")
  }

  @Test("under Reduce Motion the window fades out, lands at once and fades in")
  func reduceMotionNeverTravels() {
    for resizes in [true, false] {
      let steps = GlideSteps(reduceMotion: true, resizes: resizes)
      #expect(steps.fading == .window)
      #expect(steps.travel == 0)
      #expect(steps.fadeOut <= Motion.Duration.fade.seconds && steps.fadeIn <= Motion.Duration.fade.seconds)
    }
  }

  @Test("a move reads Reduce Motion as it starts, and counts as gliding until it lands")
  func aMoveFollowsReduceMotion() {
    let window = NSWindow(contentRect: .zero, styleMask: [.borderless], backing: .buffered, defer: true)
    window.isReleasedWhenClosed = false
    /// The person's setting, which the test turns off between moves, and the moves asked for.
    final class Record {
      var reduced = true
      var steps: [GlideSteps] = []
      var landings: [@MainActor () -> Void] = []
    }
    let record = Record()
    let glide = WindowGlide(
      window: window,
      reduceMotion: { record.reduced },
      animate: { _, _, steps, landed in
        record.steps.append(steps)
        record.landings.append(landed)
      }
    )
    let target = NSRect(x: 40, y: 60, width: 300, height: 200)
    glide.move(to: target)
    #expect(record.steps.last?.fading == .window)
    #expect(glide.isGliding)
    record.landings.removeFirst()()
    #expect(!glide.isGliding)
    record.reduced = false
    glide.move(to: target.offsetBy(dx: 100, dy: 0))
    #expect(record.steps.last?.fading == .content, "the borderless window starts at no size, so this one resizes")
    #expect(glide.isGliding)
  }
}
