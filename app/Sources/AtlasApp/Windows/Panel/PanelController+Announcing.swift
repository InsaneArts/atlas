import AtlasStatus

/// What VoiceOver hears from the open panel: only a state that asks something of the person, once it has held for
/// ``PanelController/announceAfter``, as in the main window. Working and Idle, and every reading that says the same
/// again, say nothing.
extension PanelController {
  /// What VoiceOver should say about the helper now: its word and line while it asks something of the person (it
  /// needs them, agents are stopped, they have control, it is logged out or not ready) and the panel is open; nil
  /// otherwise, as for every Working and Idle.
  var askingStatus: String? {
    guard isOpen else { return nil }
    let mode = mode
    switch mode.kind {
    case .needsYou, .stopped, .control, .loggedOut, .notReady:
      let status = mode.panelSummary(StatusReading(status), paused: session.paused, reconnecting: answered == false)
      return status.line.isEmpty ? status.label : "\(status.label), \(status.line)"
    case .idle, .working, .inUseElsewhere, .connecting:
      return nil
    }
  }

  /// Says `text` once it has held for ``announceAfter`` on the panel's clock; a newer one, or none, calls it off. The
  /// same text again changes nothing.
  ///
  /// - Parameter text: What VoiceOver should say now, from ``askingStatus``.
  func announceSoon(_ text: String?) {
    guard text != askedBefore else { return }
    askedBefore = text
    announcing?.cancel()
    announcing = nil
    guard let text else {
      if session.announcement != nil { session.announcement = nil }
      return
    }
    let clock = clock
    // Unstructured, as the state settles while the panel goes on.
    announcing = Task { [weak self] in
      try? await clock.sleep(for: Self.announceAfter)
      guard let self, !Task.isCancelled else { return }
      session.announcement = text
    }
  }

  /// Calls off what waits to be said, and takes the state the panel opens on as already heard, since VoiceOver reads
  /// the panel as it opens.
  func restartAnnouncements() {
    announcing?.cancel()
    announcing = nil
    if session.announcement != nil { session.announcement = nil }
    askedBefore = askingStatus
  }
}
