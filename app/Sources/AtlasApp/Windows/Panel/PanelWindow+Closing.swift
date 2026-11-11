import AppKit
import AtlasUI

extension PanelWindow {
  /// Closes the panel, as a click elsewhere, Escape, its item, or opening a window or an alert from it does. It fades
  /// away over ``Motion/Duration/contentOut``, as a menu does, and goes at once under Reduce Motion; either way it is
  /// closed from now, so its item opens it again.
  func closePanel() {
    guard isOpen else { return }
    isOpen = false
    turn += 1
    closedAt = ProcessInfo.processInfo.systemUptime
    stopWatchingClicks()
    stopWatchingKeys()
    item.showsPanel(false)
    let closing = turn
    let seconds = Motion.loops(reduceMotion: Appearance.reduceMotion) ? Motion.Duration.contentOut.seconds : 0
    show(opacity: 0, over: seconds) { [weak self] in
      // Opened again while it faded: that opening owns the window now.
      guard let self, self.turn == closing else { return }
      self.closed()
    }
  }

  /// The panel has gone: the window leaves the screen whole again for its next opening, and reading slows to the
  /// closed rate.
  private func closed() {
    close()
    show(opacity: 1, over: 0)
    controller.watch(open: false)
  }

  /// Takes the window to an opacity, at once or fading, ending whatever fade was running.
  ///
  /// - Parameters:
  ///   - opacity: The opacity to reach.
  ///   - seconds: How long the fade takes; zero lands at once.
  ///   - done: Runs once it is there.
  func show(opacity: CGFloat, over seconds: TimeInterval, then done: @escaping @MainActor () -> Void = {}) {
    NSAnimationContext.runAnimationGroup { context in
      context.duration = seconds
      context.timingFunction = CAMediaTimingFunction(name: .easeOut)
      animator().alphaValue = opacity
    } completionHandler: {
      // AppKit runs an animation group's completion handler on the main thread, where the group began, so the main
      // actor is already running here; assumeIsolated says so rather than hopping.
      MainActor.assumeIsolated { done() }
    }
  }

  /// Closes the panel on a click anywhere but on its card or its item, until the panel closes: in another app, in
  /// another of Atlas's windows, or in the clear room round the card; and when Atlas stops being the app in front.
  func watchClicks() {
    stopWatchingClicks()
    let downs: NSEvent.EventTypeMask = [.leftMouseDown, .rightMouseDown, .otherMouseDown]
    outsideClicks = NSEvent.addGlobalMonitorForEvents(matching: downs) { [weak self] _ in
      let point = NSEvent.mouseLocation
      // Unstructured, as a monitor's handler is synchronous: the panel closes after it, on the main actor. A click on
      // its own item is left to the item, which closes it: the menu bar may hand that click to another process first.
      Task { @MainActor in
        guard let self, self.anchor?.contains(point) != true else { return }
        self.closePanel()
      }
    }
    ownClicks = NSEvent.addLocalMonitorForEvents(matching: downs) { [weak self] event in
      // A local monitor runs on the main thread, as the event loop does.
      MainActor.assumeIsolated { self?.clicked(event) }
      return event
    }
    leaving = NotificationCenter.default
      .addObserver(forName: NSApplication.didResignActiveNotification, object: nil, queue: .main) { [weak self] _ in
        MainActor.assumeIsolated { self?.closePanel() }
      }
  }

  /// A click in one of Atlas's windows while the panel shows: the panel closes unless the click is on its card, or on
  /// its item, which closes it itself. The ⋯ menu's own clicks are the menu's.
  ///
  /// - Parameter event: The click.
  private func clicked(_ event: NSEvent) {
    guard isOpen, !controller.session.moreOpen else { return }
    if event.window === self {
      let card = PanelWindowFrame.card(in: frame.size)
      if !card.contains(event.locationInWindow) { closePanel() }
    } else if event.window !== item.button?.window {
      closePanel()
    }
  }

  /// Stops watching for clicks elsewhere and for Atlas leaving the front, once the panel closes.
  func stopWatchingClicks() {
    if let outsideClicks { NSEvent.removeMonitor(outsideClicks) }
    outsideClicks = nil
    if let ownClicks { NSEvent.removeMonitor(ownClicks) }
    ownClicks = nil
    if let leaving {
      NotificationCenter.default
        .removeObserver(leaving)
    }
    leaving = nil
  }
}
