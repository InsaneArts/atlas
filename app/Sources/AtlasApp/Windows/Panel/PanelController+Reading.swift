import AtlasCommands
import AtlasService
import AtlasUI

/// The readings: the status at the open or closed rate, the picture while the panel is open and not paused, Open at
/// Login and the helper's account. Each reading is numbered, so only the newest one started is shown.
extension PanelController {
  /// Reads the status now and then at the open or closed rate, and the picture at the open rate while open and not
  /// paused, until called again. The two run apart, so a slow capture never holds the status back. Opening also reads
  /// Open at Login and the helper's account again, since Getting Started or the system may have changed them
  /// meanwhile, and drops what the last action said. Opening or closing starts VoiceOver's announcements afresh.
  ///
  /// - Parameter open: Whether the panel is open.
  func watch(open: Bool) {
    stop()
    let turning = open != isOpen
    isOpen = open
    if turning { restartAnnouncements() }
    if open {
      session.notice = nil
      readOpenAtLogin()
      polling.append(Task { [weak self] in await self?.findHelper() })
    }
    polling.append(repeating(every: open ? Self.openEvery : Self.closedEvery) { await $0.readStatus() })
    if open && !session.paused { polling.append(repeating(every: Self.openEvery) { await $0.readFrame() }) }
  }

  /// Stops every reading under way.
  private func stop() {
    for task in polling {
      task.cancel()
    }
    polling = []
  }

  /// Runs `step` now and then every `pause`, until cancelled or the controller is gone.
  ///
  /// - Parameters:
  ///   - pause: The pause between steps.
  ///   - step: What to run.
  /// - Returns: The task running it.
  private func repeating(
    every pause: Duration,
    _ step: @escaping @MainActor (PanelController) async -> Void
  )
    -> Task<Void, Never>
  {
    return Task { [weak self, clock] in
      while !Task.isCancelled {
        if let controller = self { await step(controller) } else { return }
        // A cancelled pause ends at once, and the loop's condition ends the loop.
        try? await clock.sleep(for: pause)
      }
    }
  }

  /// Reads where Open at Login stands now: the ⋯ menu ticks it, and its item turns it from there. While a change
  /// runs the tick keeps what the person asked for; the change reads it back once it ends.
  func readOpenAtLogin() {
    guard !changingAtLogin else { return }
    opensAtLogin = loginItem.state
  }

  /// Learns the helper to switch to: whichever account `atlas` reports. A listing that fails, or was cancelled,
  /// keeps the one known, so one failure never loses the helper for good.
  func findHelper() async {
    let listing = await commands.run(.listAccounts)
    guard !Task.isCancelled, listing.ok, let accounts = listing.decode(AccountList.self) else { return }
    listedHelper = accounts.helper
  }

  /// Reads the status. A reading that was cancelled, or overtaken by a newer one, shows nothing, and so does a status
  /// the service refused to give (it was busy, say): the last one stands rather than reading as unavailable.
  func readStatus() async {
    statusReading += 1
    let number = statusReading
    let latest: ViewerStatus?
    do {
      latest = try await service.status()
    } catch is StatusRefused {
      return
    } catch {
      latest = nil
    }
    guard number == statusReading, !Task.isCancelled else { return }
    status = latest ?? ViewerStatus()
    answered = latest != nil
    links.read(latest)
  }

  /// Reads the picture, under the same rule, decoding it off the main actor (``ImageDecoding``); a reading overtaken
  /// or cancelled while it decodes shows nothing either.
  func readFrame() async {
    frameReading += 1
    let number = frameReading
    // A failed capture is nil, and shows no picture rather than an old one.
    let shown = try? await service.frame()
    guard number == frameReading, !Task.isCancelled else { return }
    var decoded: DesktopPicture?
    if let shown, let image = await ImageDecoding.image(shown.png) {
      decoded = DesktopPicture(id: shown.id, image: image)
    }
    guard number == frameReading, !Task.isCancelled else { return }
    // A failed capture never leaves an old screen on show.
    picture = decoded
  }
}
