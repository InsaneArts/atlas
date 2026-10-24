import AtlasService
import AtlasUI
import Foundation

/// Reading: the status, then a picture, one reading at a time while the window is on screen and not paused.
extension ViewerController {
  /// Read while the window is on screen and not paused; stop otherwise.
  ///
  /// - Parameter on: Whether to read.
  func watch(_ on: Bool) {
    watching = on
    generation += 1
    // One reading at a time across restarts too: the next loop starts
    // once the last one has ended.
    let previous = polling
    previous?.cancel()
    napping?.cancel()
    polling = nil
    if on {
      let number = generation
      polling = Task { [weak self] in
        await previous?.value
        _ = await self?.readWhileWatching(number)
      }
    }
    tick(on)
  }

  /// Read again now, or right after the reading under way.
  func refresh() {
    readAgain = true
    napping?.cancel()
  }

  /// One reading at a time; each waits for the last to end, and the pause between them is the rate's, or
  /// ``controlEvery`` while holding control.
  ///
  /// - Parameter number: The loop's generation; a newer one ends it.
  private func readWhileWatching(_ number: Int) async {
    while watching && number == generation && !Task.isCancelled {
      readAgain = false
      await read(number)
      guard watching, number == generation, !Task.isCancelled else { return }
      if readAgain { continue }
      let pause = holdingControl ? Self.controlEvery : .seconds(rate.seconds)
      // Its own task, so ``refresh()`` cancels the pause alone, not the loop; a cancelled pause ends at once.
      let resting = Task<Void, Never> { [clock] in try? await clock.sleep(for: pause) }
      napping = resting
      await resting.value
      napping = nil
    }
  }

  /// The status, then a picture. A reading overtaken by a newer one, or by
  /// a pause or close, has no effect at all, failures included, and so has a
  /// status the service refused to give (it was busy, say): what shows stays.
  ///
  /// - Parameter number: The loop's generation.
  func read(_ number: Int) async {
    statusReading += 1
    let readingNumber = statusReading
    let current = { [unowned self] in watching && number == generation && readingNumber == statusReading }
    // With no picture yet, the first one is asked for alongside the
    // status, so the window shows the helper at once.
    async let early = reading.frame == nil ? (try? await service.frame()) : nil
    do {
      let reply = try await service.status()
      guard current() else { return }
      show(reply)
      if let reason = reply.unobservable?.word { throw ServiceError(reason) }
      let next: Frame
      if let early = await early {
        next = early
      } else {
        next = try await service.frame()
      }
      guard current() else { return }
      // Decoded once: the view draws its pixels, and input is mapped through its size.
      guard let decoded = DesktopPicture.decode(next) else { throw ServiceError.unknownScreenSize }
      show(next, decoded: decoded, ready: reply.ready)
    } catch is StatusRefused {
      return
    } catch {
      if current() { lost(error) }
    }
  }

  /// Read the status once, outside the picture loop, and show it.
  func readStatus() async {
    statusReading += 1
    let readingNumber = statusReading
    guard let reply = try? await service.status(), readingNumber == statusReading else { return }
    show(reply)
  }

  /// Refreshes ``now`` every ``tickEvery`` while readings run, so Details' age of the picture stays current; stops
  /// otherwise.
  ///
  /// - Parameter running: Whether readings run.
  private func tick(_ running: Bool) {
    ticking?.cancel()
    ticking = nil
    guard running else { return }
    ticking = Task { [weak self, clock] in
      while !Task.isCancelled {
        guard self != nil else { return }
        self?.readClock()
        try? await clock.sleep(for: Self.tickEvery)
      }
    }
  }

  /// What the panel last saw, shown while the first reading is under way. It is only a picture: control waits for a
  /// reading of its own.
  ///
  /// - Parameters:
  ///   - status: The service's last answer to the panel, or nil when it has none.
  ///   - picture: The panel's latest picture, if any.
  func begin(with status: ViewerStatus?, picture: DesktopPicture?) {
    guard !reading.hasStatus, reading.frame == nil else { return }
    reading.status = status ?? ViewerStatus()
    reading.hasStatus = status != nil
    reading.picture = picture
  }

  /// Shows a status. Input not ready ends control now, and so does a status asked after the grant that says no
  /// viewer holds it, as after Release Control in the menu bar; a recovery set aside is forgotten once nothing needs
  /// it.
  ///
  /// - Parameter reply: The status read; only the newest reading started is shown, so it is ``statusReading``'s.
  func show(_ reply: ViewerStatus) {
    let newAccount = reply.overview.account != reading.status.overview.account
    reading.status = reply
    reading.hasStatus = true
    // Details tells when the helper logged in once its account is known, and again should it change.
    if newAccount, detailsShown { readLoginTime() }
    // Input not ready: control ends now, not with the next picture, saying why in the status's own words.
    if !reply.ready {
      reading.ready = false
      if holdingControl || session.acquiring { releaseControl(ViewerWords.controlEnded(reply.overview.reason)) }
    } else if holdingControl, statusReading > controlSince, reply.noViewerInControl {
      releaseControl(ViewerWords.releasedElsewhere)
    }
    // Not Now covers one incident: once nothing needs recovery, a later one alerts again.
    if snapshot.offeredRecovery == nil {
      session.recoveryLater = nil
      // A refusal belongs to the alert it answered: once nothing needs recovery it is gone with it.
      if session.alert != .recover { session.recoveryRefusal = nil }
    }
  }

  /// Shows a new picture, read with a status that says whether the helper takes input. A picture from a changed
  /// helper ends control first, when this viewer holds or asks for it.
  ///
  /// - Parameters:
  ///   - frame: The picture's frame.
  ///   - decoded: Its picture and size, decoded once.
  ///   - ready: Whether the status read with it says the helper takes input.
  func show(_ frame: Frame, decoded: DesktopPicture.Decoded, ready: Bool) {
    if holdingControl || session.acquiring, let shown = reading.frame, shown.revision != frame.revision {
      releaseControl(ViewerWords.helperChanged)
    }
    reading.frame = frame
    reading.picture = decoded.picture
    reading.pictureSize = decoded.size
    reading.frameAt = uptime()
    reading.unavailable = nil
    reading.ready = ready
  }

  /// No picture: say why, and give control back.
  ///
  /// - Parameter error: Why the reading failed.
  func lost(_ error: any Error) {
    reading.ready = false
    reading.frame = nil
    reading.picture = nil
    reading.pictureSize = nil
    releaseControl()
    reading.unavailable = error.localizedDescription
  }
}
