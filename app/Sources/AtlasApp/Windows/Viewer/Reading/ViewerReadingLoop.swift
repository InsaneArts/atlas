import AtlasCommands
import AtlasService
import AtlasUI
import Foundation
import Observation

/// Reading: the status, then a picture unless paused, one reading at a time while the window is on screen, and the
/// picture on screen, copied or saved as it is, never captured anew. ``ViewerController`` owns one; its state is
/// written here alone, each part only when it changes, so a reading that changes nothing presents nothing. A picture
/// that changes only the picture reaches the live slots alone (``feed``).
@Observable
final class ViewerReadingLoop {
  /// Between pictures while holding control: 500 ms, so the person sees what their input did at once.
  static let controlEvery: Duration = .milliseconds(500)
  /// How often the time is read again while pictures arrive: 1 s, as a picture's age reads in whole seconds.
  static let tickEvery: Duration = .seconds(1)

  /// What the window is laid out by from the last reading, observed. Its picture changes only as a picture comes or
  /// goes, or comes at another size, and its frame only as a frame comes or goes, or names another display; the newest
  /// picture and frame are unobserved (``newestPicture``, ``newestFrame``).
  private var shown = ViewerReading()
  /// The newest picture, which only the live slots observe; every picture replaces it.
  let feed = LivePictureFeed()
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
  /// The newest picture, as the feed has it, unobserved here so reading it follows nothing.
  @ObservationIgnored private var newestPicture: DesktopPicture?
  /// The newest frame: its id, revision and display, which input is aimed by. Unobserved, as every frame replaces it.
  @ObservationIgnored private var newestFrame: (id: String, revision: String, display: ViewerStatus.Size)?
  /// The newest picture's size in points, which input is mapped through.
  @ObservationIgnored private var newestSize: CGSize?
  /// When the newest picture arrived, in seconds of uptime.
  @ObservationIgnored private var newestAt: TimeInterval?
  /// The time last read, in seconds of uptime, which a picture's age is told by: refreshed each ``tickEvery`` while
  /// pictures arrive. Unobserved: a tick presents nothing, and an age is read as the window presents.
  @ObservationIgnored private(set) var now: TimeInterval = 0
  /// Whether readings run: the window is on screen.
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
  /// Waits ``Motion/Duration/appearDelay`` for a picture before the waiting stage rings its mark.
  @ObservationIgnored private var waiting: Task<Void, Never>?
  /// Streams pictures from the frame service while watching, when the app can reach one.
  @ObservationIgnored private let pump: ViewerFramePump?

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
    pump = dependencies.framePipe.map {
      ViewerFramePump(service: dependencies.service, connector: $0, clock: dependencies.clock)
    }
    rate = ViewerPictureRate(rawValue: preferences.loadRate()) ?? .live
  }

  /// What the last reading showed: what the window is laid out by (``shown``), with the newest frame's facts as they
  /// are now, which input is aimed and mapped by. Whoever reads it follows only the first, so a picture that changes
  /// nothing else presents nothing.
  var reading: ViewerReading {
    var reading = shown
    reading.frame = newestFrame
    reading.pictureSize = newestSize
    reading.frameAt = newestAt
    return reading
  }

  /// Read while the window is on screen; stop otherwise.
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
    if on && viewer?.session.paused != true { awaitPicture() } else { stopAwaiting() }
    if on {
      pump?.start(
        paused: { [weak self] in self?.viewer?.session.paused ?? true },
        pause: { [weak self] in
          // Live, or controlling, streams at the service's pace; a slower rate waits between pictures.
          guard let self, rate != .live, viewer?.holdingControl != true else { return .zero }
          return .seconds(rate.seconds)
        },
        show: { [weak self] frame, decoded in
          guard let self else { return }
          show(frame, decoded: decoded, ready: reading.status.ready)
        }
      )
    } else {
      pump?.stop()
    }
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

  /// The status, then a picture unless paused. A reading overtaken by a newer one, or by
  /// a pause or close, has no effect at all, failures included, and so has a
  /// status the service refused to give (it was busy, say): what shows stays.
  ///
  /// - Parameters:
  ///   - number: The loop's generation.
  ///   - decode: Decodes the picture off the main actor.
  func read(_ number: Int, decode: (Frame) async -> DesktopPicture.Decoded? = DesktopPicture.decode) async {
    statusReading += 1
    let readingNumber = statusReading
    let soundAsked = viewer?.actions.sound.generation
    let current = { [unowned self] in watching && number == generation && readingNumber == statusReading }
    let paused = viewer?.session.paused == true
    // While the frame service streams pictures, a reading reads the status alone.
    let streaming = pump?.streaming == true
    let needsEarlyFrame = !paused && newestFrame == nil && !streaming
    // With no picture yet, the first one is asked for alongside the
    // status, so the window shows the helper at once.
    async let early = needsEarlyFrame ? (try? await service.frame()) : nil
    do {
      let reply = try await service.status()
      guard current() else { return }
      show(reply, soundAsked: soundAsked)
      guard !paused else {
        if shown.unavailable != nil { shown.unavailable = nil }
        return
      }
      if let reason = reply.unobservable?.word { throw ServiceError(reason) }
      if streaming { return }
      let next: Frame
      if let early = await early {
        next = early
      } else {
        next = try await service.frame()
      }
      guard current(), pump?.streaming != true else { return }
      // Decoded once: the view draws its pixels, and input is mapped through its size.
      let decoded = await decode(next)
      // The frame service may have taken over meanwhile: its pictures win.
      guard current(), pump?.streaming != true else { return }
      guard let decoded else { throw ServiceError.unknownScreenSize }
      show(next, decoded: decoded, ready: reply.ready)
    } catch is StatusRefused {
      return
    } catch {
      if current() && !paused { lost(error) }
    }
  }

  /// Read the status once, outside the picture loop, and show it.
  func readStatus() async {
    statusReading += 1
    let readingNumber = statusReading
    let soundAsked = viewer?.actions.sound.generation
    // A status that cannot be read leaves the window as it is; the reading loop reads it again.
    guard let reply = try? await service.status(), readingNumber == statusReading else { return }
    show(reply, soundAsked: soundAsked)
  }

  /// Refreshes ``now`` every ``tickEvery`` while readings run, so the picture's age stays current; stops otherwise.
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
        // A cancelled tick ends at once, and the loop's condition ends the loop.
        try? await clock.sleep(for: Self.tickEvery)
      }
    }
  }

  /// Reads the clock into ``now``; the window presents again only if the time changed what it shows
  /// (``ViewerController/clockMoved()``).
  func readClock() {
    now = uptime()
    viewer?.clockMoved()
  }

  /// What the panel last saw, shown while the first reading is under way. It is only a picture: control waits for a
  /// reading of its own.
  ///
  /// - Parameters:
  ///   - status: The service's last answer to the panel, or nil when it has none.
  ///   - picture: The panel's latest picture, if any.
  func begin(with status: ViewerStatus?, picture: DesktopPicture?) {
    guard !shown.hasStatus, newestFrame == nil else { return }
    shown.status = status ?? ViewerStatus()
    shown.hasStatus = status != nil
    shown.picture = picture
    shown.lastPicture = picture
    newestPicture = picture
    if let picture { feed.show(picture) }
  }

  /// Shows a status. Input not ready ends control now, and so does a status asked after the grant that says no
  /// viewer holds it, as after Release Control in the menu bar; a recovery set aside is forgotten once nothing needs
  /// it. The same status again changes nothing the window shows.
  ///
  /// - Parameters:
  ///   - reply: The status read; only the newest reading started is shown, so it is ``statusReading``'s.
  ///   - soundAsked: The helper sound switch's generation when the status was asked for; nil shows no sound.
  func show(_ reply: ViewerStatus, soundAsked: Int? = nil) {
    let newAccount = reply.overview.account != shown.status.overview.account
    if shown.status != reply { shown.status = reply }
    if !shown.hasStatus { shown.hasStatus = true }
    guard let viewer else { return }
    if let soundAsked {
      viewer.actions.sound.adopt(reply.overview.sound.flatMap(HelperSound.init), asked: soundAsked)
    }
    // The helper's facts tell when it logged in once its account is known, and again should it change.
    if newAccount { viewer.readLoginTime() }
    // Input not ready: control ends now, not with the next picture, saying why in the status's own words.
    let control = viewer.control
    if !reply.ready {
      if shown.ready { shown.ready = false }
      if viewer.holdingControl || control.acquiring {
        control.suspend(ViewerWords.controlEnded(reply.overview.reason))
      }
    } else if viewer.holdingControl, statusReading > control.controlSince, reply.noViewerInControl {
      control.suspend(ViewerWords.releasedElsewhere)
    }
    viewer.forgetRecoveryOnceNoneNeeded()
    // Discover and refresh session history; the pane model decides when live sessions also need polling.
    viewer.panesModel.followSessions()
  }

  /// Shows a new picture, read with a status that says whether the helper takes input. A picture from a changed
  /// helper ends control first, when this viewer holds or asks for it. The picture goes to the live slots (``feed``);
  /// the window presents again only when it comes at another size or its frame names another display.
  ///
  /// - Parameters:
  ///   - frame: The picture's frame.
  ///   - decoded: Its picture and size, decoded once.
  ///   - ready: Whether the status read with it says the helper takes input.
  func show(_ frame: Frame, decoded: DesktopPicture.Decoded, ready: Bool) {
    // A change between the viewer's pictures and the frame service's is a change of transport, not of helper:
    // their revisions are compared only with their own kind.
    if let viewer, viewer.holdingControl || viewer.control.acquiring, let newest = newestFrame,
      newest.revision.hasPrefix("pipe:") == frame.revision.hasPrefix("pipe:"), newest.revision != frame.revision
    {
      viewer.control.suspend(ViewerWords.helperChanged)
    }
    stopAwaiting()
    let wasStale = reading.pictureStale(at: now)
    setFrame((frame.id, frame.revision, frame.display))
    newestSize = decoded.size
    newestAt = uptime()
    newestPicture = decoded.picture
    feed.show(decoded.picture)
    if !laysOutLike(decoded.picture) {
      shown.picture = decoded.picture
      shown.lastPicture = decoded.picture
    }
    if shown.unavailable != nil { shown.unavailable = nil }
    if shown.ready != ready { shown.ready = ready }
    // A fresh picture after a stale one may offer control again at once, not at the clock's next tick.
    if wasStale { viewer?.clockMoved() }
    viewer?.control.pictureArrived()
  }

  /// No picture: say why, and give control back. The newest picture drawn stays behind the words.
  ///
  /// - Parameter error: Why the reading failed.
  func lost(_ error: any Error) {
    if shown.ready { shown.ready = false }
    setFrame(nil)
    if let newest = drawnPicture, shown.lastPicture?.id != newest.id { shown.lastPicture = newest }
    if shown.picture != nil { shown.picture = nil }
    newestPicture = nil
    newestSize = nil
    viewer?.control.suspend()
    let reason = error.localizedDescription
    if shown.unavailable != reason { shown.unavailable = reason }
    awaitPicture()
  }

  /// The newest frame, and the window's frame with it while it comes, goes or names another display: the window is
  /// laid out by whether there is one and its display, never by which one it is.
  ///
  /// - Parameter frame: The newest frame, or nil for none.
  private func setFrame(_ frame: (id: String, revision: String, display: ViewerStatus.Size)?) {
    newestFrame = frame
    if shown.frame?.display != frame?.display { shown.frame = frame }
  }

  /// Whether `picture` draws at the size of the picture the window was laid out with, so it changes nothing but the
  /// picture.
  ///
  /// - Parameter picture: A new picture.
  /// - Returns: Whether it does; false while the window has no picture.
  private func laysOutLike(_ picture: DesktopPicture) -> Bool {
    guard let laidOut = shown.picture else { return false }
    return laidOut.image.width == picture.image.width && laidOut.image.height == picture.image.height
  }

  /// The picture on screen now, the newest the live slots draw; nil while there is none.
  private var drawnPicture: DesktopPicture? {
    shown.picture.map { newestPicture ?? $0 }
  }

  /// Rings the waiting stage's mark once the window has waited ``Motion/Duration/appearDelay`` for a picture,
  /// on the window's clock, so a quick answer never flashes it. It starts as reading starts with no picture
  /// and as the picture is lost; a picture's arrival, or reading stopping, ends the wait and puts the ring
  /// away.
  private func awaitPicture() {
    guard shown.picture == nil, waiting == nil, !shown.waitedLong else { return }
    // Unstructured, as the wait runs while readings go on; cancelled the moment a picture arrives.
    waiting = Task { [weak self, clock] in
      try? await clock.sleep(for: .seconds(Motion.Duration.appearDelay.seconds))
      guard let self, !Task.isCancelled else { return }
      waiting = nil
      if shown.picture == nil { shown.waitedLong = true }
    }
  }

  /// Ends the wait for a picture, if one runs, and puts the ring away: the next wait, as reading starts again or a
  /// picture is lost again, waits its whole moment before it rings.
  private func stopAwaiting() {
    waiting?.cancel()
    waiting = nil
    if shown.waitedLong { shown.waitedLong = false }
  }

  /// Resuming the picture reads afresh: the last picture is not one to aim at, nor is the helper known to take input,
  /// until a reading says so. The picture stays drawn meanwhile.
  func forgetFrame() {
    if shown.ready { shown.ready = false }
    setFrame(nil)
    newestAt = nil
  }

  /// Copy Picture: the frame on screen goes to the clipboard.
  func copyPicture() {
    guard let picture = drawnPicture else { return }
    pictures.copy(picture.image)
    viewer?.say(ViewerWords.pictureCopied)
  }

  /// Save Picture…: asks where, then writes the frame on screen as a PNG. Nothing is asked of the helper.
  func savePicture() {
    guard let picture = drawnPicture else { return }
    if pictures.save(picture.image) == false { viewer?.say(ViewerWords.pictureNotSaved) }
  }
}
