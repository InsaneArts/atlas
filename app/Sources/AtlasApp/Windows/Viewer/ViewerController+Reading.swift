import AtlasKit
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
      let resting = Task { [nap = self.nap] in await nap(pause) }
      napping = resting
      await resting.value
      napping = nil
    }
  }

  /// The status, then a picture. A reading overtaken by a newer one, or by
  /// a pause or close, has no effect at all, failures included.
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
      if let reason = reply.unobservableReason { throw ServiceError(reason) }
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

  /// Refreshes ``now`` every ``tickEvery`` while readings run, so the header's age of the picture stays current;
  /// stops otherwise. A paused picture's header reads no age.
  ///
  /// - Parameter running: Whether readings run.
  private func tick(_ running: Bool) {
    ticking?.cancel()
    ticking = nil
    guard running else { return }
    ticking = Task { [weak self] in
      while !Task.isCancelled {
        guard self != nil else { return }
        self?.readClock()
        try? await Task.sleep(for: Self.tickEvery)
      }
    }
  }
}
