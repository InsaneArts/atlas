import AppKit
import AtlasUI
import SwiftUI

/// The onboarding window: a dark card in the middle of the screen, or a narrow panel at its left while the person
/// turns a switch on in System Settings beside it. It has a Dock icon while open, like any app window (``DockIcon``).
@MainActor
final class OnboardingWindow: NSObject, NSWindowDelegate {
  /// The glide's curve, its control points: an ease in and out steeper than AppKit's own, so the window leaves softly,
  /// travels quickly and lands softly over ``Motion/Duration/glide``.
  static let glideCurve = (x1: Float(0.65), y1: Float(0), x2: Float(0.35), y2: Float(1))

  /// The window's controller.
  let controller: OnboardingController
  /// The window.
  let window: NSWindow
  /// Runs once the window has closed and its controller has stopped.
  let closed: @MainActor (OnboardingWindow) -> Void
  /// Open until closed.
  private(set) var isOpen = false
  /// Whether the window was last placed for the guide panel; nil before the first placing. The controller's state
  /// changes with every reading, and the window moves only when the placing asked for differs from this one
  /// (``OnboardingPlacement/placed(guiding:last:place:)``).
  private var placedGuiding: Bool?
  /// Whether the window takes the keyboard now.
  var isKey: Bool { window.isKeyWindow }

  /// Creates the window, hidden until ``show()``.
  ///
  /// - Parameters:
  ///   - controller: The window's controller.
  ///   - closed: Runs once the window has closed and its controller has stopped.
  init(controller: OnboardingController, closed: @escaping @MainActor (OnboardingWindow) -> Void) {
    self.controller = controller
    self.closed = closed
    window = NSWindow(
      contentRect: NSRect(origin: .zero, size: OnboardingPlacement.size(guiding: false)),
      styleMask: [.titled, .closable, .fullSizeContentView],
      backing: .buffered,
      defer: false
    )
    super.init()
    window.title = "Atlas"
    window.titleVisibility = .hidden
    window.titlebarAppearsTransparent = true
    window.isReleasedWhenClosed = false
    window.isMovableByWindowBackground = true
    window.appearance = NSAppearance(named: .darkAqua)
    window.backgroundColor = .ground
    window.delegate = self
    let host = NSHostingView(
      rootView: ShellRoot {
        OnboardingView(
          state: controller.state,
          form: Binding(get: { controller.snapshot.form }, set: { controller.update($0) }),
          send: controller.send
        )
      }
      .environment(\.appIcon, AppIconImage.image)
    )
    host.sizingOptions = []
    window.contentView = host
    StateTracking.follow(
      { [weak controller] in controller.map { $0.snapshot.guiding != nil } },
      apply: { [weak self] guiding in
        guard let self else { return }
        placedGuiding = OnboardingPlacement.placed(guiding: guiding, last: placedGuiding, place: place(guiding:))
      }
    )
    StateTracking.follow(
      { [weak controller] in controller?.closeRequested },
      apply: { [weak self] requested in
        if requested { self?.window.close() }
      }
    )
  }

  /// Brings the window forward, reading the facts while it is open. The app shows its Dock icon first.
  func show() {
    if !isOpen {
      window.setContentSize(OnboardingPlacement.size(guiding: false))
      window.center()
    }
    isOpen = true
    window.makeKeyAndOrderFront(nil)
    NSApp.activate()
    controller.watch()
  }

  /// Moves to the card in the middle, or the side panel at the left of the screen, floating so it stays in view.
  ///
  /// - Parameter guiding: Whether the guide panel shows.
  /// - Returns: Whether it moved: a hidden window, or one with no screen, does not.
  @discardableResult
  func place(guiding: Bool) -> Bool {
    guard window.isVisible, let screen = window.screen ?? NSScreen.main else { return false }
    let content = NSRect(origin: .zero, size: OnboardingPlacement.size(guiding: guiding))
    let frame = window.frameRect(forContentRect: content)
    let origin = OnboardingPlacement.origin(guiding: guiding, frame: frame.size, visible: screen.visibleFrame)
    window.level = guiding ? .floating : .normal
    let curve = Self.glideCurve
    NSAnimationContext.runAnimationGroup { context in
      context.duration = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion ? 0 : Motion.Duration.glide.seconds
      context.timingFunction = CAMediaTimingFunction(controlPoints: curve.x1, curve.y1, curve.x2, curve.y2)
      window
        .animator()
        .setFrame(NSRect(origin: origin, size: frame.size), display: true)
    }
    return true
  }

  /// Closes the window.
  func close() { window.close() }

  /// The window is closing: its controller stops, and the app settles its Dock icon once `closed` has run.
  ///
  /// - Parameter notification: The window's notification.
  func windowWillClose(_ notification: Notification) {
    isOpen = false
    Task {
      await controller.close()
      closed(self)
    }
  }
}
