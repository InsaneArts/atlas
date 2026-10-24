import AtlasService
import AtlasUI
import Foundation
import Observation

/// Reading: the status, then a picture, one reading at a time while the window is on screen and not paused, and the
/// picture on screen, copied or saved as it is, never captured anew. ``ViewerController`` owns one; its state is
/// written here alone.
@Observable
final class ViewerReadingLoop {
  /// Between pictures while holding control: 500 ms, so the person sees what their input did at once.
  static let controlEvery: Duration = .milliseconds(500)
  /// How often Details' age of the picture is read again while pictures arrive: 1 s, as it reads in whole seconds.
  static let tickEvery: Duration = .seconds(1)

  /// What the last reading showed.
  private(set) var reading = ViewerReading()
  /// Counts the clock's readings. Only ``ViewerController/newestPicture`` reads it, so each tick redraws Details' age
  /// row alone.
  private(set) var ticks = 0
  /// How often a picture is read while watching.
  private(set) var rate = ViewerPictureRate.live {
    didSet {
      preferences.saveRate(rate.seconds)
      refresh()
    }
  }

  /// The window this loop reads for: whether it holds control, and what it is doing. Weak, as the window owns it.
  @ObservationIgnored private weak var viewer: ViewerController?
  /// The viewer service it reads.
  @ObservationIgnored private let service: any ViewerService
  /// Where the rate is kept between windows.
  @ObservationIgnored private let preferences: ViewerPreferences
  /// The time now, in seconds of uptime, which never steps back: what a picture's age is told by.
  @ObservationIgnored private let uptime: @MainActor () -> TimeInterval
  /// The clock every wait runs on; a cancelled wait ends at once.
  @ObservationIgnored private let clock: any Clock<Duration>
  /// Where a copied or saved picture goes.
  @ObservationIgnored private let pictures: PictureOutlet
  /// The time Details last read, in seconds of uptime: refreshed each ``tickEvery`` while pictures arrive.
  /// Unobserved: the window redraws with each reading, and Details' age row follows ``ticks``.
  @ObservationIgnored private(set) var now: TimeInterval = 0
  /// Whether readings run: the window is on screen and not paused.
  @ObservationIgnored private(set) var watching = false
  /// Numbers each start and stop of reading, so a stopped loop ends.
  @ObservationIgnored private(set) var generation = 0
  /// Numbers each status reading, so only the newest one started is shown.
  @ObservationIgnored private(set) var statusReading = 0
  /// The reading loop.
  @ObservationIgnored private(set) var polling: Task<Void, Never>?
  /// The pause between readings, cancelled to read again at once.
  @ObservationIgnored private var napping: Task<Void, Never>?
  /// A reading was asked for while one ran: done as soon as it ends.
  @ObservationIgnored private(set) var readAgain = false
  /// Refreshes ``now`` while readings run.
  @ObservationIgnored private(set) var ticking: Task<Void, Never>?

  /// Creates a loop that has read nothing yet, at the person's saved rate.
  ///
  /// - Parameters:
  ///   - viewer: The window it reads for.
  ///   - dependencies: What it reaches outside the window; a saved rate the window does not offer reads as Live.
  init(viewer: ViewerController, dependencies: ViewerDependencies) {
    self.viewer = viewer
    service = dependencies.service
    preferences = dependencies.preferences
    uptime = dependencies.uptime
    clock = dependencies.clock
    pictures = dependencies.pictures
    rate = ViewerPictureRate(rawValue: preferences.loadRate()) ?? .live
  }

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

  /// Picks how often a picture is read while watching.
  ///
  /// - Parameter rate: The rate.
  func setRate(_ rate: ViewerPictureRate) {
    self.rate = rate
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
      let pause = viewer?.holdingControl == true ? Self.controlEvery : .seconds(rate.seconds)
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

  /// Reads the clock into ``now``, and tells Details' age row.
  func readClock() {
    now = uptime()
    ticks += 1
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
    guard let viewer else { return }
    // Details tells when the helper logged in once its account is known, and again should it change.
    if newAccount, viewer.details.shown { viewer.readLoginTime() }
    // Input not ready: control ends now, not with the next picture, saying why in the status's own words.
    let control = viewer.control
    if !reply.ready {
      reading.ready = false
      if viewer.holdingControl || control.acquiring {
        control.releaseControl(ViewerWords.controlEnded(reply.overview.reason))
      }
    } else if viewer.holdingControl, statusReading > control.controlSince, reply.noViewerInControl {
      control.releaseControl(ViewerWords.releasedElsewhere)
    }
    viewer.forgetRecoveryOnceNoneNeeded()
    viewer.noteActivitySeen()
  }

  /// Shows a new picture, read with a status that says whether the helper takes input. A picture from a changed
  /// helper ends control first, when this viewer holds or asks for it.
  ///
  /// - Parameters:
  ///   - frame: The picture's frame.
  ///   - decoded: Its picture and size, decoded once.
  ///   - ready: Whether the status read with it says the helper takes input.
  func show(_ frame: Frame, decoded: DesktopPicture.Decoded, ready: Bool) {
    if let viewer, viewer.holdingControl || viewer.control.acquiring, let shown = reading.frame,
      shown.revision != frame.revision
    {
      viewer.control.releaseControl(ViewerWords.helperChanged)
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
    viewer?.control.releaseControl()
    reading.unavailable = error.localizedDescription
  }

  /// Resuming the picture reads afresh: the last picture is not one to aim at, nor is the helper known to take input,
  /// until a reading says so. The picture stays drawn meanwhile.
  func forgetFrame() {
    reading.ready = false
    reading.frame = nil
    reading.frameAt = nil
  }

  /// Copy Picture: the frame on screen goes to the clipboard.
  func copyPicture() {
    guard let picture = reading.picture else { return }
    pictures.copy(picture.image)
    viewer?.say(ViewerWords.pictureCopied)
  }

  /// Save Picture…: asks where, then writes the frame on screen as a PNG. Nothing is asked of the helper.
  func savePicture() {
    guard let picture = reading.picture else { return }
    if pictures.save(picture.image) == false { viewer?.say(ViewerWords.pictureNotSaved) }
  }
}
