import AppKit
import AsyncSupportTesting
import AtlasUI
import Testing

@testable import AtlasApp

@MainActor
@Suite("SettingsDock")
struct SettingsDockTests {
  /// A see-through window that takes no clicks, ordered in so the dock counts it as shown, docked beside a sample
  /// System Settings window, its first glide ended.
  final class Docked {
    /// The panel's window.
    let window: NSWindow
    /// The glides it was given.
    let glides = HeldGlides()
    /// System Settings' window frame, which a test moves.
    let settings: SettingsFrame
    /// The clock the looks wait on.
    let clock = ManualClock()
    /// Whether a mouse button is down, as the test says.
    let pointer = HeldPointer()
    /// The screen's visible frame.
    let visible: CGRect
    /// The ghost the dock shows.
    let ghost = DragGhost()
    /// The panel's frame size.
    let size = OnboardingPlacement.size(guiding: true)
    /// The dock under test.
    let dock: SettingsDock

    /// Docks the panel.
    ///
    /// - Throws: A failed requirement with no screen.
    init() throws {
      let screen = try #require(NSScreen.main, "a screen to dock on")
      visible = screen.visibleFrame
      window = NSWindow(
        contentRect: NSRect(origin: visible.origin, size: OnboardingPlacement.size(guiding: false)),
        styleMask: [.borderless],
        backing: .buffered,
        defer: false
      )
      window.isReleasedWhenClosed = false
      window.alphaValue = 0
      window.ignoresMouseEvents = true
      window.orderFrontRegardless()
      settings = SettingsFrame(CGRect(x: visible.minX + 40, y: visible.maxY - 560, width: 600, height: 520))
      let glide = WindowGlide(window: window, reduceMotion: { false }, motion: glides.motion)
      dock = SettingsDock(
        window: window,
        glide: glide,
        clock: clock,
        ghost: ghost,
        settingsFrame: { [settings] in settings.frame },
        waitingFor: { nil },
        lost: {},
        mouseDown: { [pointer] in pointer.isDown }
      )
      dock.start(size: size, visible: visible)
      glides.endAll()
    }

    /// Where the panel belongs beside System Settings now.
    var place: CGPoint {
      OnboardingPlacement.docked(frame: size, beside: settings.frame, visible: visible)
    }

    /// Ends the dock and the window.
    func close() {
      dock.close()
      ghost.close()
      window.orderOut(nil)
    }
  }

  @Test("the first move into the dock glides, and following a short move of System Settings moves at once")
  func theFirstMoveGlidesAndFollowingIsInstant() async throws {
    let docked = try Docked()
    defer { docked.close() }
    #expect(docked.glides.travels.count == 1, "the window glides into the dock")
    #expect(docked.glides.contentFades == [0, 1], "its content fades around the glide")
    let first = try #require(docked.glides.travels.first)
    #expect(first.origin == docked.place)

    // The person moves System Settings a little: at the next look the window is beside it again, set there.
    docked.settings.frame = docked.settings.frame.offsetBy(dx: 12, dy: -10)
    try await docked.clock.untilSleeping(for: SettingsDock.followEvery, "the dock waits for its next look")
    docked.clock.advance(by: SettingsDock.followEvery)
    try await docked.clock.untilSleeping(for: SettingsDock.followEvery, "the dock looked and waits again")
    #expect(docked.window.frame.origin == docked.place)
    #expect(docked.glides.travels.count == 1, "following never glides")
  }

  @Test("while the person drags System Settings, the panel follows it every frame, not every eighth of a second")
  func draggingIsFollowedEveryFrame() async throws {
    let docked = try Docked()
    defer { docked.close() }
    try await docked.clock.untilSleeping(for: SettingsDock.followEvery, "the dock waits for its next look")
    docked.pointer.isDown = true
    docked.clock.advance(by: SettingsDock.followEvery)
    for step in 1...6 {
      try await docked.clock.untilSleeping(for: SettingsDock.followFrame, "the dock never looked a frame later")
      docked.settings.frame = docked.settings.frame.offsetBy(dx: 8, dy: 0)
      docked.clock.advance(by: SettingsDock.followFrame)
      try await docked.clock.untilSleeping(for: SettingsDock.followFrame, "the dock stopped following")
      #expect(docked.window.frame.origin == docked.place, "frame \(step) of the drag")
    }
  }

  @Test("after a jump, the panel catches up on a spring with no bounce rather than snapping")
  func aJumpIsCaughtUpOnASpring() async throws {
    let docked = try Docked()
    defer { docked.close() }
    let start = docked.window.frame.origin
    docked.settings.frame = docked.settings.frame.offsetBy(dx: 0, dy: -200)
    try await docked.clock.untilSleeping(for: SettingsDock.followEvery, "the dock waits for its next look")
    docked.clock.advance(by: SettingsDock.followEvery)
    try await docked.clock.untilSleeping(for: SettingsDock.followFrame, "the dock never caught up frame by frame")
    let after = docked.window.frame.origin
    #expect(after.y < start.y && after.y > docked.place.y, "part of the way, not snapped: \(after)")
    var last = after.y
    for _ in 0..<60 where docked.window.frame.origin != docked.place {
      docked.clock.advance(by: SettingsDock.followFrame)
      try await docked.clock.untilSleeping(1, for: nil, "the dock stopped")
      #expect(docked.window.frame.origin.y <= last, "it never turns back, as a bounce would")
      #expect(docked.window.frame.origin.y >= docked.place.y, "it never overshoots")
      last = docked.window.frame.origin.y
    }
    #expect(docked.window.frame.origin == docked.place, "it lands in its place")
  }

  @Test("a short move is taken at once, a long one on the spring until it lands, and the spring never overshoots")
  func followingRule() {
    var follow = DockFollow()
    let target = CGPoint(x: 100, y: 100)
    #expect(follow.next(from: CGPoint(x: 90, y: 100), toward: target, after: 1.0 / 60) == target)
    let first = follow.next(from: CGPoint(x: 100, y: 300), toward: target, after: 1.0 / 60)
    #expect(first.y < 300 && first.y > 100 && follow.isCatchingUp)
    var point = first
    for _ in 0..<60 where follow.isCatchingUp {
      let next = follow.next(from: point, toward: target, after: 1.0 / 60)
      #expect(next.y <= point.y && next.y >= target.y)
      point = next
    }
    #expect(point == target && !follow.isCatchingUp)
  }
}
