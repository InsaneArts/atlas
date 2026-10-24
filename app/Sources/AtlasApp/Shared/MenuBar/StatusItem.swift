import AppKit
import AtlasStatus
import AtlasUI

/// Atlas's item in the menu bar: it owns the system's status item and its button, which the panel's popover opens
/// from (``PanelPopover``). One width in every state (v3 M1), so the items to its left never move; the button shows
/// the orbit (``StatusGlyph``) and a ``StatusBadge`` layer over it.
///
/// A state change moves (v3 M4, X14): the badge pops and crossfades, and the orbit's dim fades over
/// ``Motion/Duration/dim`` as the button's own layer, which holds the template image, changes its opacity, so the
/// menu bar still inks the template. Under Reduce Motion, and while a menu is open, a new state swaps in at once
/// (``StatusMotion``); a menu opening stops whatever a state change set moving, so nothing moves in the frame its
/// tracking starts (v3 R16). Once a change's animations end, the button draws again, so a screen that only
/// composites what the app commits, such as the off-console helper's, shows the final state.
final class StatusItem {
  /// The item's measures.
  enum Layout {
    /// 22: the item's width in every state, replacing the variable length that grew with a mark.
    static let width: CGFloat = 22
  }

  /// The dim's key on the button's layer.
  static let dimKey = "dim"

  /// The badge, on the button's layer.
  let badge = StatusBadge()
  /// The status bar's item, kept in its bar while this lives.
  private let item: NSStatusItem
  /// Whether Reduce Motion is on, read each time the item shows a state or lays its badge out.
  private let reduceMotion: () -> Bool
  /// Whether the main run loop is tracking events, as it does while a menu is open.
  private let tracking: () -> Bool
  /// The look shown now; nil before the first.
  private var look: StatusGlyph.Look?
  /// How many menus have begun tracking and not yet ended, so a state change while one is open swaps in at once (v3
  /// R16). It counts only while the run loop tracks, so a lost end cannot hold it open.
  private var menusOpen = 0
  /// The watches that lay the badge out again when the menu bar's appearance or the display's scale changes.
  private var watches: [NSKeyValueObservation] = []
  /// The watches for the button's frame changing, Reduce Motion being turned on or off, and menus opening and
  /// closing, each with the centre it was added to.
  private var observers: [(centre: NotificationCenter, token: any NSObjectProtocol)] = []

  /// The item's button: the panel's anchor, showing the helper's mode.
  var button: NSStatusBarButton? { item.button }
  /// The item's width in its bar.
  var length: CGFloat { item.length }

  /// Adds the item to a status bar, its badge layer on the button.
  ///
  /// - Parameters:
  ///   - bar: The status bar: the system's menu bar, or a bar of its own in tests, which never shows.
  ///   - reduceMotion: Reads whether Reduce Motion is on.
  ///   - tracking: Reads whether the main run loop is tracking events, as it does while a menu is open.
  init(
    bar: NSStatusBar = .system,
    reduceMotion: @escaping () -> Bool = { Appearance.reduceMotion },
    tracking: @escaping () -> Bool = { RunLoop.main.currentMode == .eventTracking }
  ) {
    item = bar.statusItem(withLength: Layout.width)
    self.reduceMotion = reduceMotion
    self.tracking = tracking
    observers = [
      watch(NSMenu.didBeginTrackingNotification, of: nil, in: .default) { item in item.menuOpened() },
      watch(NSMenu.didEndTrackingNotification, of: nil, in: .default) { item in item.menuClosed() },
    ]
    guard let button = item.button else { return }
    button.wantsLayer = true
    guard let host = button.layer else { return }
    host.addSublayer(badge.layer)
    badge.finished = { [weak button] in button?.needsDisplay = true }
    button.postsFrameChangedNotifications = true
    watches = [
      button.observe(\.effectiveAppearance) { [weak self] _, _ in
        MainActor.assumeIsolated { self?.showBadge() }
      },
      host.observe(\.contentsScale) { [weak self] _, _ in
        MainActor.assumeIsolated { self?.showBadge() }
      },
    ]
    observers += [
      watch(NSView.frameDidChangeNotification, of: button, in: .default) { item in item.showBadge() },
      watch(
        NSWorkspace.accessibilityDisplayOptionsDidChangeNotification,
        of: nil,
        in: NSWorkspace.shared.notificationCenter
      ) { item in item.showBadge() },
    ]
  }

  isolated deinit {
    for observer in observers {
      observer.centre
        .removeObserver(observer.token)
    }
  }

  /// What may move now: nothing under Reduce Motion, only a breath that runs while a menu is open, otherwise
  /// everything.
  var motion: StatusMotion {
    StatusMotion.now(reduceMotion: reduceMotion(), menuOpen: menuOpen())
  }

  /// Whether a menu is open: one has begun tracking and the run loop still tracks. Read outside tracking, the count
  /// starts again from none, so a menu whose end was lost cannot hold the item still.
  ///
  /// - Returns: Whether a menu is open.
  private func menuOpen() -> Bool {
    guard tracking() else {
      menusOpen = 0
      return false
    }
    return menusOpen > 0
  }

  /// A menu began tracking: whatever a state change set moving stops where its model values are, final, and the
  /// breath keeps running.
  private func menuOpened() {
    menusOpen += 1
    badge.settle()
    button?.layer?.removeAnimation(forKey: Self.dimKey)
  }

  /// A menu ended tracking: the badge is laid out again, so a breath its look asks for starts now.
  private func menuClosed() {
    menusOpen = max(0, menusOpen - 1)
    showBadge()
  }

  /// Shows a mode: its look, and the words the tooltip and VoiceOver say. A new look fades the orbit's dim and pops
  /// or crossfades the badge, as ``motion`` allows.
  ///
  /// - Parameters:
  ///   - mode: The helper's mode.
  ///   - description: The tooltip and accessibility description, which agree.
  func show(_ mode: Mode.Kind, description: String) {
    let look = StatusGlyph.look(for: mode)
    guard look != self.look || button?.toolTip != description else { return }
    let image = StatusGlyph.image(for: look)
    image.accessibilityDescription = description
    button?.image = image
    button?.toolTip = description
    let first = self.look == nil
    self.look = look
    dim(to: StatusGlyph.opacity(of: look.tint), fades: !first && motion == .all)
    showBadge()
  }

  /// Fades the orbit to an opacity: the button's own layer, which holds the template image, so the menu bar keeps
  /// inking it. At once when it may not fade, stopping a fade that runs. Once a fade ends the button draws again.
  ///
  /// - Parameters:
  ///   - opacity: The orbit's opacity, ``StatusGlyph/dimAlpha`` or whole.
  ///   - fades: Whether the change may animate.
  private func dim(to opacity: CGFloat, fades: Bool) {
    guard let button, let host = button.layer else { return }
    let shown = (host.presentation() ?? host).opacity
    host.removeAnimation(forKey: Self.dimKey)
    button.alphaValue = opacity
    guard fades, shown != Float(opacity) else { return }
    let fade = CABasicAnimation(keyPath: "opacity")
    fade.fromValue = shown
    fade.toValue = Float(opacity)
    fade.duration = Motion.Duration.dim.seconds
    fade.timingFunction = CAMediaTimingFunction(name: .easeOut)
    // Drawn again once the fade ends, as the badge is (``StatusBadge/finished``).
    CATransaction.begin()
    CATransaction.setCompletionBlock { [weak button] in
      MainActor.assumeIsolated { button?.needsDisplay = true }
    }
    host.add(fade, forKey: Self.dimKey)
    CATransaction.commit()
  }

  /// Lays the badge out for the look shown now: on the button's image, on whole pixels of its display, in the menu
  /// bar's appearance. Only a change of badge moves it, so laying the same one out again adds no animation.
  func showBadge() {
    guard let look, let button, let host = button.layer else { return }
    // A layer added by hand keeps one pixel a point unless told the display's scale, which AppKit keeps on the host.
    badge.layer.contentsScale = host.contentsScale
    let image =
      (button.cell as? NSButtonCell)?
      .imageRect(forBounds: button.bounds)
      ?? NSRect(
        x: (button.bounds.width - Layout.width) / 2,
        y: (button.bounds.height - Layout.width) / 2,
        width: Layout.width,
        height: Layout.width
      )
    badge.show(
      look,
      in: image,
      flipped: button.isFlipped,
      appearance: button.effectiveAppearance,
      motion: motion
    )
    badge.layer.frame = button.backingAlignedRect(badge.layer.frame, options: .alignAllEdgesNearest)
  }

  /// Calls back whenever a notification comes.
  ///
  /// - Parameters:
  ///   - name: The notification.
  ///   - object: Whose notification, or nil for anyone's.
  ///   - centre: The centre that posts it.
  ///   - handle: What to do with the item, while it lives.
  /// - Returns: The centre and the observer's token, to remove it.
  private func watch(
    _ name: Notification.Name,
    of object: Any?,
    in centre: NotificationCenter,
    handle: @escaping @MainActor (StatusItem) -> Void
  ) -> (centre: NotificationCenter, token: any NSObjectProtocol) {
    let token = centre.addObserver(forName: name, object: object, queue: .main) { [weak self] _ in
      MainActor.assumeIsolated {
        guard let self else { return }
        handle(self)
      }
    }
    return (centre, token)
  }
}
