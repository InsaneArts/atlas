import AppKit
import AsyncSupportTesting
import AtlasUI
import Testing

@testable import AtlasApp

@MainActor
@Suite("SettingsDock")
struct SettingsDockTests {
  @Test("the first move into the dock glides, and following System Settings moves at once")
  func theFirstMoveGlidesAndFollowingIsInstant() async throws {
    let screen = try #require(NSScreen.main, "a screen to dock on")
    let visible = screen.visibleFrame
    // A see-through window that takes no clicks, ordered in so the dock counts it as shown.
    let window = NSWindow(
      contentRect: NSRect(origin: visible.origin, size: OnboardingPlacement.size(guiding: false)),
      styleMask: [.borderless],
      backing: .buffered,
      defer: false
    )
    window.isReleasedWhenClosed = false
    window.alphaValue = 0
    window.ignoresMouseEvents = true
    window.orderFrontRegardless()
    defer { window.orderOut(nil) }
    let glides = HeldGlides()
    let glide = WindowGlide(window: window, reduceMotion: { false }, motion: glides.motion)
    let settings = SettingsFrame(
      CGRect(x: visible.minX + 40, y: visible.maxY - 560, width: 600, height: 520)
    )
    let clock = ManualClock()
    let ghost = DragGhost()
    defer { ghost.close() }
    let dock = SettingsDock(
      window: window,
      glide: glide,
      clock: clock,
      ghost: ghost,
      settingsFrame: { settings.frame },
      waitingFor: { nil },
      lost: {}
    )
    defer { dock.close() }
    let size = OnboardingPlacement.size(guiding: true)

    dock.start(size: size, visible: visible)
    glides.endAll()
    #expect(glides.travels.count == 1, "the window glides into the dock")
    #expect(glides.contentFades == [0, 1], "its content fades around the glide")
    let first = try #require(glides.travels.first)
    #expect(first.origin == OnboardingPlacement.docked(frame: size, beside: settings.frame, visible: visible))

    // The person moves System Settings: at the next look the window is beside it again, set there, not glided.
    settings.frame = settings.frame.offsetBy(dx: 60, dy: -30)
    try await clock.untilSleeping(for: SettingsDock.followEvery, "the dock waits for its next look")
    clock.advance(by: SettingsDock.followEvery)
    try await clock.untilSleeping(for: SettingsDock.followEvery, "the dock looked and waits again")
    #expect(window.frame.origin == OnboardingPlacement.docked(frame: size, beside: settings.frame, visible: visible))
    #expect(glides.travels.count == 1, "following never glides")
  }
}
