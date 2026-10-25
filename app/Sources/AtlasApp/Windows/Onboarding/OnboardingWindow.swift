import AppKit
import AtlasUI
import SwiftUI

/// The onboarding window: a dark card in the middle of the screen, or a narrow panel beside System Settings, following
/// its window, while the person turns a switch on there (``SettingsDock``). It has a Dock icon while open, like any app
/// window (``DockIcon``). Built on the one shell (``WindowShell``), and opened and closed by the coordinator
/// (``WindowPresenting``); the glide, the dock and the drag ghost's overlay are its own, and close with it.
final class OnboardingWindow: NSWindow, NSWindowDelegate, WindowPresenting {
  /// The window's controller.
  let controller: OnboardingController
  /// Runs once the window has closed and its controller has stopped.
  let closed: @MainActor (OnboardingWindow) -> Void
  /// Open until closed.
  private(set) var isPresented = false
  /// Whether the window was last placed for the permission panel; nil before the first placing. The controller's state
  /// changes with every reading, and the window moves only when the placing asked for differs from this one
  /// (``OnboardingPlacement/placed(guiding:last:place:)``).
  private var placedGuiding: Bool?
  /// The controller's `run()`, its reading loop and its queue, from the first `present()`; it ends by itself once the
  /// controller has closed and a request under way has settled.
  private var work: Task<Void, Never>?
  /// The drag ghost, which the permission panel's icon starts and the dock shows over System Settings.
  private let ghost = DragGhost()
  /// Moves the window. Made once the window exists, as it moves this window.
  private lazy var glide = WindowGlide(window: self)
  /// Keeps the permission panel beside System Settings. Made once the window exists, as it moves this window.
  private lazy var dock = SettingsDock(
    window: self,
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

  /// Creates the window, hidden until ``present()``.
  ///
  /// - Parameters:
  ///   - controller: The window's controller.
  ///   - closed: Runs once the window has closed and its controller has stopped.
  init(controller: OnboardingController, closed: @escaping @MainActor (OnboardingWindow) -> Void) {
    self.controller = controller
    self.closed = closed
    super.init(
      contentRect: NSRect(origin: .zero, size: OnboardingPlacement.size(guiding: false)),
      styleMask: [.titled, .closable, .fullSizeContentView],
      backing: .buffered,
      defer: false
    )
    title = "Getting Started"
    titleVisibility = .hidden
    titlebarAppearsTransparent = true
    isReleasedWhenClosed = false
    isMovableByWindowBackground = true
    delegate = self
    let ghost = ghost
    WindowShell.host(in: self) {
      OnboardingView(
        state: controller.state,
        form: Binding(get: { controller.snapshot.form }, set: { controller.update($0) }),
        dragIcon: AppDragIcon(app: controller.dependencies.app, help: OnboardingWords.dragHelp, ghost: ghost),
        send: controller.send
      )
    }
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
        if requested { self?.close() }
      }
    )
  }

  /// Brings the window forward, its controller reading the facts and performing its requests while it is open. The app
  /// shows its Dock icon first.
  func present() {
    if !isPresented {
      setContentSize(OnboardingPlacement.size(guiding: false))
      center()
    }
    isPresented = true
    makeKeyAndOrderFront(nil)
    NSApp.activate()
    if work == nil {
      // Unstructured, as AppKit's present() cannot wait: the controller runs while the window is open, and its run()
      // ends by itself once the window has closed.
      work = Task { [controller] in await controller.run() }
    }
  }

  /// Moves to the card in the middle, coming in front again, or to the side panel beside System Settings, floating so
  /// it stays in view while the dock follows System Settings' window.
  ///
  /// - Parameter guiding: Whether the permission panel shows.
  /// - Returns: Whether it moved: a hidden window, or one with no screen, does not.
  @discardableResult
  func place(guiding: Bool) -> Bool {
    dock.stop()
    level = guiding ? .floating : .normal
    guard isVisible, let screen = screen ?? NSScreen.main else { return false }
    let content = NSRect(origin: .zero, size: OnboardingPlacement.size(guiding: guiding))
    let size = frameRect(forContentRect: content).size
    guard !guiding else {
      dock.start(size: size, visible: screen.visibleFrame)
      return true
    }
    let origin = OnboardingPlacement.origin(guiding: false, frame: size, visible: screen.visibleFrame)
    glide.move(to: NSRect(origin: origin, size: size))
    // Back from System Settings, which may still have the focus: the card comes in front of it all the same (macOS
    // lets an app take the focus only when the person acts in it), and takes the focus where macOS allows.
    orderFrontRegardless()
    makeKey()
    NSApp.activate()
    return true
  }

  /// Closes the window.
  func dismiss() { close() }

  /// The window is closing: its controller stops, and the app settles its Dock icon once `closed` has run.
  ///
  /// - Parameter notification: The window's notification.
  func windowWillClose(_ notification: Notification) {
    isPresented = false
    dock.close()
    // Unstructured, as AppKit's notification cannot wait: the controller's close runs to its end, then `closed`.
    Task {
      await controller.close()
      closed(self)
    }
  }
}
