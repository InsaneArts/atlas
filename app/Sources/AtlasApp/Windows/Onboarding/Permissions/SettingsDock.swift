import AppKit

/// Keeps the permission panel beside System Settings' window while it guides, and shows the drag ghost there while
/// the panel waits for its switch. System Settings may take a moment to open: the panel waits where the card was, then
/// glides beside it once (or to the screen's left should it show no window by then), and moves with it after: every
/// frame while a mouse button is down, as the person drags System Settings, at once while its place is near, and on a
/// spring with no bounce after a jump (``DockFollow``).
final class SettingsDock {
  /// How often the panel looks for System Settings' window to follow it: 8 times a second, so it keeps up with a
  /// window the person moves.
  static let followEvery: Duration = .milliseconds(125)
  /// How often the panel follows System Settings while the person drags it, or while the panel catches up: one frame
  /// of a 60 Hz display, so it moves with the window rather than snapping after it.
  static let followFrame: Duration = .seconds(1.0 / 60)
  /// How many looks the panel waits for System Settings' window before going to the screen's left: 8, about a second.
  static let settingsPatience = 8
  /// How many looks the ghost waits once the panel sits beside System Settings, or its page changed: 8, about a
  /// second, so System Settings has drawn its list before the ghost points into it.
  static let ghostPatience = 8

  /// The setup window, which owns this dock and so outlives it.
  private unowned let window: NSWindow
  /// Moves the window.
  private let glide: WindowGlide
  /// The clock the looks wait on.
  private let clock: any Clock<Duration>
  /// The ghost showing the person where to drag Atlas.
  private let ghost: DragGhost
  /// Where System Settings' window is, in AppKit's coordinates; nil when it shows none.
  private let settingsFrame: @MainActor () -> CGRect?
  /// The permission the panel waits for, by its label; nil once its switch is on, while its tick shows.
  private let waitingFor: @MainActor () -> String?
  /// Runs once System Settings, docked beside, shows no window for ``settingsPatience`` looks: closed or quit.
  private let lost: @MainActor () -> Void
  /// Whether a mouse button is down now: the person dragging a window, or dragging Atlas in.
  private let mouseDown: @MainActor () -> Bool
  /// How the panel keeps to its place, look by look.
  private var follow = DockFollow()
  /// Following System Settings' window, while the panel guides.
  private var following: Task<Void, Never>?
  /// Whether the window sits beside System Settings: following it from there is instant, not a glide.
  private var docked = false
  /// Whether the panel went to the screen's left, System Settings having shown no window in time.
  private var leftAside = false
  /// Looks in a row, once docked, that found no System Settings window.
  private var missed = 0
  /// The permission the ghost shows the way to, by its label.
  private var shown: String?
  /// Looks since the panel docked beside System Settings, or its permission changed.
  private var settled = 0

  /// Creates the dock, idle until ``start(size:visible:)``.
  ///
  /// - Parameters:
  ///   - window: The setup window.
  ///   - glide: Moves the window.
  ///   - clock: The clock the looks wait on; the setup window's.
  ///   - ghost: The drag ghost.
  ///   - settingsFrame: Where System Settings' window is.
  ///   - waitingFor: The permission the panel waits for, by its label.
  ///   - lost: Runs once System Settings, docked beside, is closed or quit.
  ///   - mouseDown: Whether a mouse button is down now; the Mac's own by default.
  init(
    window: NSWindow,
    glide: WindowGlide,
    clock: any Clock<Duration>,
    ghost: DragGhost,
    settingsFrame: @escaping @MainActor () -> CGRect? = SystemSettingsWindow.frame,
    waitingFor: @escaping @MainActor () -> String?,
    lost: @escaping @MainActor () -> Void,
    mouseDown: @escaping @MainActor () -> Bool = { NSEvent.pressedMouseButtons != 0 }
  ) {
    self.window = window
    self.glide = glide
    self.clock = clock
    self.ghost = ghost
    self.settingsFrame = settingsFrame
    self.waitingFor = waitingFor
    self.lost = lost
    self.mouseDown = mouseDown
  }

  /// Moves the panel beside System Settings, or, while it shows no window yet, shrinks it where the card was; then
  /// follows.
  ///
  /// - Parameters:
  ///   - size: The panel's frame size.
  ///   - visible: The visible frame of the window's screen.
  func start(size: CGSize, visible: CGRect) {
    stop()
    if let origin = settingsFrame().flatMap({ origin(size: size, beside: $0) }) {
      docked = true
      glide.move(to: NSRect(origin: origin, size: size))
    } else {
      glide.move(
        to: NSRect(origin: OnboardingPlacement.shrunk(frame: size, from: window.frame, visible: visible), size: size)
      )
    }
    // Unstructured and stored, as the window's placing cannot wait: it follows until `stop()` cancels it.
    following = Task { [weak self, clock] in
      var looks = 0
      var sinceLook = Duration.zero
      while !Task.isCancelled {
        let step = self?.isFollowingClosely == true ? Self.followFrame : Self.followEvery - sinceLook
        // Cancelled while asleep: no step after, which would dock a card just put back.
        do { try await clock.sleep(for: step) } catch { return }
        guard let self else { return }
        sinceLook += step
        guard sinceLook >= Self.followEvery else {
          keepUp(after: step)
          continue
        }
        // A whole look: System Settings checked for, the ghost shown or hidden, as every eighth of a second.
        sinceLook = .zero
        looks += 1
        catchUp(waited: looks >= Self.settingsPatience, after: step)
      }
    }
  }

  /// Stops for good, with the window: the ghost's overlay closes.
  func close() {
    stop()
    ghost.close()
  }

  /// Stops following and hides the ghost.
  func stop() {
    following?.cancel()
    following = nil
    ghost.hide()
    docked = false
    leftAside = false
    missed = 0
    shown = nil
    settled = 0
    follow = DockFollow()
  }

  /// Whether the panel follows System Settings frame by frame now: docked, while a mouse button is down or while it
  /// catches up after a jump.
  private var isFollowingClosely: Bool {
    docked && (mouseDown() || follow.isCatchingUp)
  }

  /// One frame between looks: the panel keeps to System Settings' window, nothing else.
  ///
  /// - Parameter elapsed: The time since the last frame or look.
  private func keepUp(after elapsed: Duration) {
    guard window.isVisible, docked, !glide.isGliding, let settings = settingsFrame(),
      let origin = origin(size: window.frame.size, beside: settings)
    else { return }
    move(toward: origin, after: elapsed)
  }

  /// Moves the docked panel toward its place, at once when it is near, else on the catching-up spring.
  ///
  /// - Parameters:
  ///   - origin: Where it belongs.
  ///   - elapsed: The time since the last move.
  private func move(toward origin: CGPoint, after elapsed: Duration) {
    let distance = hypot(origin.x - window.frame.minX, origin.y - window.frame.minY)
    guard distance >= OnboardingPlacement.followSlack || follow.isCatchingUp else { return }
    let seconds = Double(elapsed.components.seconds) + Double(elapsed.components.attoseconds) / 1e18
    window.setFrameOrigin(follow.next(from: window.frame.origin, toward: origin, after: seconds))
  }

  /// One look of following, the ghost shown a moment after the panel sits beside System Settings while its switch is
  /// off, and hidden while a mouse button is down (the person moving a window, or dragging Atlas in) or the window
  /// glides. With a mouse button down the panel still keeps to System Settings.
  ///
  /// - Parameters:
  ///   - waited: Whether System Settings has had its time to show a window.
  ///   - elapsed: The time since the last frame or look.
  private func catchUp(waited: Bool, after elapsed: Duration) {
    guard window.isVisible, !glide.isGliding else { return ghost.hide() }
    guard !mouseDown() else {
      ghost.hide()
      return keepUp(after: elapsed)
    }
    let size = window.frame.size
    let settings = settingsFrame()
    defer { demonstrate(settings) }
    guard let settings, let origin = origin(size: size, beside: settings) else {
      if docked {
        missed += 1
        if missed >= Self.settingsPatience { lost() }
      } else if waited, !leftAside, let screen = window.screen {
        leftAside = true
        let origin = OnboardingPlacement.origin(guiding: true, frame: size, visible: screen.visibleFrame)
        glide.move(to: NSRect(origin: origin, size: size))
      }
      return
    }
    missed = 0
    if docked {
      move(toward: origin, after: elapsed)
    } else if hypot(origin.x - window.frame.minX, origin.y - window.frame.minY) >= OnboardingPlacement.followSlack {
      docked = true
      glide.move(to: NSRect(origin: origin, size: size))
    }
  }

  /// Shows the ghost into `settings` once the panel has sat beside it for ``ghostPatience`` looks on this permission,
  /// and starts its loops again for each new permission.
  ///
  /// - Parameter settings: System Settings' window frame.
  private func demonstrate(_ settings: CGRect?) {
    let permission = docked && !glide.isGliding ? waitingFor() : nil
    if permission != shown {
      shown = permission
      settled = 0
      ghost.restart()
    }
    // System Settings showing no window for a moment (a page changing) waits its moment again.
    guard permission != nil, settings != nil else {
      settled = 0
      return ghost.hide()
    }
    settled += 1
    ghost.show(into: settled > Self.ghostPatience ? settings : nil)
  }

  /// Where the panel sits beside `settings`, on the screen holding most of it.
  ///
  /// - Parameters:
  ///   - size: The panel's frame size.
  ///   - settings: System Settings' window frame.
  /// - Returns: The frame's origin; nil with no screen.
  private func origin(size: CGSize, beside settings: CGRect) -> CGPoint? {
    let overlap = { (screen: NSScreen) in
      let shared = screen.frame.intersection(settings)
      return shared.isNull ? 0 : shared.width * shared.height
    }
    guard let screen = NSScreen.screens.max(by: { overlap($0) < overlap($1) }) ?? window.screen else { return nil }
    return OnboardingPlacement.docked(frame: size, beside: settings, visible: screen.visibleFrame)
  }
}
