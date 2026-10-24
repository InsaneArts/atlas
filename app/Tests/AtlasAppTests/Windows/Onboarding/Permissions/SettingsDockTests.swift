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
    let glides = Glides()
    let glide = WindowGlide(window: window, reduceMotion: { false }, animate: glides.land)
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
    #expect(glides.moves.count == 1, "the window glides into the dock")
    let first = try #require(glides.moves.first)
    #expect(first.steps == GlideSteps(reduceMotion: false, resizes: true), "its content fades around the glide")
    #expect(first.frame.origin == OnboardingPlacement.docked(frame: size, beside: settings.frame, visible: visible))

    // The person moves System Settings: at the next look the window is beside it again, set there, not glided.
    settings.frame = settings.frame.offsetBy(dx: 60, dy: -30)
    try await clock.untilSleeping(for: SettingsDock.followEvery, "the dock waits for its next look")
    clock.advance(by: SettingsDock.followEvery)
    try await clock.untilSleeping(for: SettingsDock.followEvery, "the dock looked and waits again")
    #expect(window.frame.origin == OnboardingPlacement.docked(frame: size, beside: settings.frame, visible: visible))
    #expect(glides.moves.count == 1, "following never glides")
  }
}

/// Where System Settings' window is, as the test moves it.
@MainActor
private final class SettingsFrame {
  /// The frame now.
  var frame: CGRect

  /// Creates the frame.
  ///
  /// - Parameter frame: Where it starts.
  init(_ frame: CGRect) {
    self.frame = frame
  }
}

/// Records each move a glide is asked for and lands it at once, as a glide that has run to its end.
@MainActor
private final class Glides {
  /// The moves, in order.
  var moves: [(frame: NSRect, steps: GlideSteps)] = []

  /// Records a move and lands the window there.
  ///
  /// - Parameters:
  ///   - window: The window.
  ///   - frame: Where it lands.
  ///   - steps: The move's steps.
  ///   - landed: Called once it has landed.
  func land(_ window: NSWindow, _ frame: NSRect, _ steps: GlideSteps, _ landed: @escaping @MainActor () -> Void) {
    moves.append((frame, steps))
    window.setFrame(frame, display: false)
    landed()
  }
}
