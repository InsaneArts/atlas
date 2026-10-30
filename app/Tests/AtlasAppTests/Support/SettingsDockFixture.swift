import AppKit
import AsyncSupportTesting
import Testing

@testable import AtlasApp

/// A see-through window that takes no clicks, ordered in so the dock counts it as shown, docked beside a sample
/// System Settings window, its first glide ended.
@MainActor
final class SettingsDockFixture {
  /// The sample System Settings window's largest size.
  static let settingsSize = CGSize(width: 600, height: 520)
  /// Its inset from the display's edges, also kept beyond the drag's end.
  static let inset: CGFloat = 40
  /// How many frames the test drives, and how far System Settings travels on each.
  static let dragFrames = 6
  static let dragStep: CGFloat = 8
  static var dragTravel: CGFloat { CGFloat(dragFrames) * dragStep }

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
  /// - Parameter reduceMotion: Whether following should move at once.
  /// - Throws: A failed requirement with no screen.
  init(reduceMotion: @escaping @MainActor () -> Bool = { false }) throws {
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
    // Leave room for the entire drag on this display, without crossing the right-to-left docking threshold.
    let width = min(
      Self.settingsSize.width,
      visible.width - size.width - OnboardingPlacement.settingsGap - Self.inset * 2 - Self.dragTravel
    )
    try #require(width > .zero, "the display has room for System Settings and the panel")
    settings = SettingsFrame(
      CGRect(
        x: visible.minX + Self.inset,
        y: visible.maxY - Self.inset - Self.settingsSize.height,
        width: width,
        height: Self.settingsSize.height
      )
    )
    let glide = WindowGlide(window: window, reduceMotion: { false }, motion: glides.motion)
    dock = SettingsDock(
      window: window,
      glide: glide,
      clock: clock,
      ghost: ghost,
      settingsFrame: { [settings] in settings.frame },
      waitingFor: { nil },
      lost: {},
      mouseDown: { [pointer] in pointer.isDown },
      reduceMotion: reduceMotion
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
