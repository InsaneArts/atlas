import AppKit
import AtlasUI
import SwiftUI

/// The onboarding window: a dark card in the middle of the screen, or a narrow panel beside System Settings, following
/// its window, while the person turns a switch on there (``SettingsDock``). It has a Dock icon while open, like any app
/// window (``DockIcon``).
final class OnboardingWindow: NSObject, NSWindowDelegate {
  /// The window's controller.
  let controller: OnboardingController
  /// The window.
  let window: NSWindow
  /// Runs once the window has closed and its controller has stopped.
  let closed: @MainActor (OnboardingWindow) -> Void
  /// Open until closed.
  private(set) var isOpen = false
  /// Whether the window was last placed for the permission panel; nil before the first placing. The controller's state
  /// changes with every reading, and the window moves only when the placing asked for differs from this one
  /// (``OnboardingPlacement/placed(guiding:last:place:)``).
  private var placedGuiding: Bool?
  /// Moves the window.
  private let glide: WindowGlide
  /// Keeps the permission panel beside System Settings.
  private let dock: SettingsDock
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
    glide = WindowGlide(window: window)
    let ghost = DragGhost()
    dock = SettingsDock(
      window: window,
      glide: glide,
      clock: controller.dependencies.clock,
      ghost: ghost,
      waitingFor: { [weak controller] in
        guard case .permissionPanel(let panel) = controller?.state.content, !panel.isOn else { return nil }
        return panel.label
      },
      // System Settings closed or quit: back to the card, whose Allow… opens it again.
      lost: { [weak controller] in controller?.send(.closePanel) }
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
          dragIcon: AppDragIcon(app: controller.dependencies.app, help: OnboardingWords.dragHelp, ghost: ghost),
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

  /// Moves to the card in the middle, coming in front again, or to the side panel beside System Settings, floating so
  /// it stays in view while the dock follows System Settings' window.
  ///
  /// - Parameter guiding: Whether the permission panel shows.
  /// - Returns: Whether it moved: a hidden window, or one with no screen, does not.
  @discardableResult
  func place(guiding: Bool) -> Bool {
    dock.stop()
    window.level = guiding ? .floating : .normal
    guard window.isVisible, let screen = window.screen ?? NSScreen.main else { return false }
    let content = NSRect(origin: .zero, size: OnboardingPlacement.size(guiding: guiding))
    let size = window.frameRect(forContentRect: content).size
    guard !guiding else {
      dock.start(size: size, visible: screen.visibleFrame)
      return true
    }
    let origin = OnboardingPlacement.origin(guiding: false, frame: size, visible: screen.visibleFrame)
    glide.move(to: NSRect(origin: origin, size: size))
    // Back from System Settings, which may still have the focus: the card comes in front of it all the same (macOS
    // lets an app take the focus only when the person acts in it), and takes the focus where macOS allows.
    window.orderFrontRegardless()
    window.makeKey()
    NSApp.activate()
    return true
  }

  /// Closes the window.
  func close() { window.close() }

  /// The window is closing: its controller stops, and the app settles its Dock icon once `closed` has run.
  ///
  /// - Parameter notification: The window's notification.
  func windowWillClose(_ notification: Notification) {
    isOpen = false
    dock.close()
    // Unstructured, as AppKit's notification cannot wait: the controller's close runs to its end, then `closed`.
    Task {
      await controller.close()
      closed(self)
    }
  }
}
