import AppKit
import AtlasUI
import Testing

@testable import AtlasApp

@MainActor
@Suite("WindowGlide", .serialized, AppKitState())
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
    let window = Self.window()
    let glides = HeldGlides()
    var reduced = true
    let glide = WindowGlide(window: window, reduceMotion: { reduced }, motion: glides.motion)
    let target = NSRect(x: 40, y: 60, width: 300, height: 200)
    glide.move(to: target)
    #expect(window.alphaValue == 0, "the whole window fades out")
    #expect(glide.isGliding)
    glides.endAll()
    #expect(window.frame == target)
    #expect(window.alphaValue == 1, "and back in once it has landed")
    #expect(!glide.isGliding)
    reduced = false
    glide.move(to: NSRect(x: 40, y: 60, width: 360, height: 240))
    #expect(glides.contentFades == [0], "a move that resizes fades the content, not the window")
    #expect(glide.isGliding)
  }

  @Test("Back during a move's fade out lands the window where Back asked, with its content shown")
  func aNewerMoveSupersedesTheOneUnderWay() {
    let card = NSRect(x: 40, y: 60, width: 300, height: 200)
    let panel = NSRect(x: 400, y: 60, width: 200, height: 400)
    let window = Self.window(card)
    let glides = HeldGlides()
    let glide = WindowGlide(window: window, reduceMotion: { false }, motion: glides.motion)
    glide.move(to: panel)
    // Back while the content fades out: the window is still the card's size, so this move glides alone.
    glide.move(to: card)
    // The first move's fade out ends after Back's move began.
    glides.endAll()
    #expect(glides.travels == [card], "the superseded move never travelled")
    #expect(window.frame == card)
    #expect(window.contentView?.alphaValue == 1, "the content the first move faded out shows again")
    #expect(!glide.isGliding)
  }

  /// A borderless window that is never shown, at `frame`.
  ///
  /// - Parameter frame: Where it is.
  /// - Returns: The window.
  private static func window(_ frame: NSRect = .zero) -> NSWindow {
    let window = NSWindow(contentRect: frame, styleMask: [.borderless], backing: .buffered, defer: true)
    window.isReleasedWhenClosed = false
    return window
  }
}
