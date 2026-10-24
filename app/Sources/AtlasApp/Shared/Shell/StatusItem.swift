import AppKit
import AtlasStatus

/// Atlas's item in the menu bar: it owns the system's status item and its button, which the panel's popover opens
/// from (``PanelPopover``). One width in every state (v3 M1), so the items to its left never move; the button shows
/// the orbit (``StatusGlyph``) and a ``StatusBadge`` layer over it.
final class StatusItem {
  /// The item's measures.
  enum Layout {
    /// 22: the item's width in every state, replacing the variable length that grew with a mark.
    static let width: CGFloat = 22
  }

  /// The badge, on the button's layer.
  let badge = StatusBadge()
  /// The status bar's item, kept in its bar while this lives.
  private let item: NSStatusItem
  /// Whether Reduce Motion is on, read each time the badge is shown.
  private let reduceMotion: () -> Bool
  /// The look shown now; nil before the first.
  private var look: StatusGlyph.Look?
  /// The watches that lay the badge out again when the menu bar's appearance or the display's scale changes.
  private var watches: [NSKeyValueObservation] = []
  /// The watches that lay the badge out again when the button's frame changes or Reduce Motion is turned on or off,
  /// each with the centre it was added to.
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
  init(bar: NSStatusBar = .system, reduceMotion: @escaping () -> Bool = { Appearance.reduceMotion }) {
    item = bar.statusItem(withLength: Layout.width)
    self.reduceMotion = reduceMotion
    guard let button = item.button else { return }
    button.wantsLayer = true
    guard let host = button.layer else { return }
    host.addSublayer(badge.layer)
    button.postsFrameChangedNotifications = true
    watches = [
      button.observe(\.effectiveAppearance) { [weak self] _, _ in
        MainActor.assumeIsolated { self?.showBadge() }
      },
      host.observe(\.contentsScale) { [weak self] _, _ in
        MainActor.assumeIsolated { self?.showBadge() }
      },
    ]
    observers = [
      watch(NSView.frameDidChangeNotification, of: button, in: .default),
      watch(
        NSWorkspace.accessibilityDisplayOptionsDidChangeNotification,
        of: nil,
        in: NSWorkspace.shared.notificationCenter
      ),
    ]
  }

  isolated deinit {
    for observer in observers {
      observer.centre
        .removeObserver(observer.token)
    }
  }

  /// Shows a mode: its look, and the words the tooltip and VoiceOver say.
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
    self.look = look
    showBadge()
  }

  /// Lays the badge out for the look shown now: on the button's image, on whole pixels of its display, in the menu
  /// bar's appearance, breathing unless Reduce Motion is on.
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
      reduceMotion: reduceMotion()
    )
    badge.layer.frame = button.backingAlignedRect(badge.layer.frame, options: .alignAllEdgesNearest)
  }

  /// Lays the badge out again whenever a notification comes.
  ///
  /// - Parameters:
  ///   - name: The notification.
  ///   - object: Whose notification, or nil for anyone's.
  ///   - centre: The centre that posts it.
  /// - Returns: The centre and the observer's token, to remove it.
  private func watch(
    _ name: Notification.Name,
    of object: Any?,
    in centre: NotificationCenter
  ) -> (centre: NotificationCenter, token: any NSObjectProtocol) {
    let token = centre.addObserver(forName: name, object: object, queue: .main) { [weak self] _ in
      MainActor.assumeIsolated { self?.showBadge() }
    }
    return (centre, token)
  }
}
