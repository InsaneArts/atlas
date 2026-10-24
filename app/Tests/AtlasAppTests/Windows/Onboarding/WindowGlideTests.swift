import AppKit
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("WindowGlide")
struct WindowGlideTests {
  @Test("the setup window moves at once under Reduce Motion")
  func theWindowMovesAtOnceUnderReduceMotion() {
    #expect(WindowGlide.duration(reduceMotion: true) == 0)
    #expect(WindowGlide.duration(reduceMotion: false) == Motion.Duration.glide.seconds)
  }

  @Test("a move reads Reduce Motion as it starts: on, the window lands at once; off, it glides")
  func aMoveFollowsReduceMotion() {
    let window = NSWindow(contentRect: .zero, styleMask: [.borderless], backing: .buffered, defer: true)
    window.isReleasedWhenClosed = false
    /// The person's setting, which the test turns off between moves.
    final class Setting {
      var reduced = true
    }
    let setting = Setting()
    let glide = WindowGlide(window: window) { setting.reduced }
    let target = NSRect(x: 40, y: 60, width: 300, height: 200)
    glide.move(to: target)
    #expect(window.frame == target)
    #expect(!glide.isGliding)
    setting.reduced = false
    glide.move(to: target.offsetBy(dx: 100, dy: 0))
    #expect(glide.isGliding)
  }
}
