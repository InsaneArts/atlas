import AtlasKit
import AtlasUI
import Foundation
import Observation

/// The viewer window's one observed object: what the last reading showed, what this window is doing (watching,
/// paused, holding control), and the person's choices. Pictures stay in memory only. Every effect is a request to the
/// viewer service, which owns the helper's safety; ``ViewerPresenter`` decides what the window shows.
@MainActor
@Observable
final class ViewerController {
  /// Between pictures while holding control: 500 ms, so the person sees what their input did at once.
  static let controlEvery: Duration = .milliseconds(500)
  /// How often the header's age of the picture is read again while pictures arrive: 1 s, as it reads in whole
  /// seconds.
  static let tickEvery: Duration = .seconds(1)
  /// One heartbeat tick: 500 ms, so a stale picture ends control within half a second; the lease is renewed every
  /// ``renewEvery`` ticks.
  static let heartbeatEvery: Duration = .milliseconds(500)
  /// The pause between readings in the app: a sleep, which cancelling ends early.
  static let sleep: @Sendable (Duration) async -> Void = { try? await Task.sleep(for: $0) }

  /// What the last reading showed.
  private(set) var reading = ViewerReading()
  /// What this window is doing.
  private(set) var session = ViewerSession()
  /// Counts the clock's readings. Only ``headerDetail`` reads it, so each tick redraws the status line alone.
  private(set) var ticks = 0

  /// How often a picture is read while watching.
  private(set) var rate = ViewerPictureRate.everyTwoSeconds {
    didSet {
      preferences.saveRate(rate.seconds)
      refresh()
    }
  }
  /// Whether the person asked for the sidebar.
  private(set) var sidebarShown = false {
    didSet { preferences.saveSidebar(sidebarShown) }
  }

  /// Where the rate and the sidebar are kept between windows.
  @ObservationIgnored let preferences: ViewerPreferences
  /// The viewer service it reads and asks.
  @ObservationIgnored let service: any ViewerService
  /// The time now, in seconds of uptime, which never steps back.
  @ObservationIgnored let clock: @MainActor () -> TimeInterval
  /// One heartbeat tick; the lease is renewed every ``renewEvery`` ticks.
  @ObservationIgnored let heartbeatInterval: Duration
  /// The pause between readings, which ends early when cancelled.
  @ObservationIgnored let nap: @Sendable (Duration) async -> Void
  /// Locks the screen so the person can log in to the helper; nil when the app cannot.
  @ObservationIgnored let logIn: (@MainActor () -> Void)?
  /// Offers the keys the Mac keeps for itself, each sent to this controller.
  @ObservationIgnored let showKeys: @MainActor (ViewerController) -> Void
  /// The time the header last read, in seconds of uptime: refreshed each ``tickEvery`` while pictures arrive.
  /// Unobserved: the window redraws with each reading and the time of the last tick, and the status line follows
  /// ``ticks``.
  @ObservationIgnored private(set) var now: TimeInterval = 0
  /// Whether readings run: the window is on screen and not paused.
  @ObservationIgnored var watching = false
  /// The window is on screen.
  @ObservationIgnored var shown = false
  /// The window takes the keyboard.
  @ObservationIgnored var focused = true

  // Control's working parts (see ViewerController+Control.swift).

  /// Numbers each acquisition and release, so a late reply knows it was overtaken.
  @ObservationIgnored var controlGeneration = 0
  /// Renews the lease and ends control over a stale picture.
  @ObservationIgnored var heartbeat: Task<Void, Never>?
  /// The mouse button held and the clicks counted.
  @ObservationIgnored var gesture = PointerGesture()
  /// Scrolling gathered and not yet sent.
  @ObservationIgnored var wheel = WheelAccumulator()
  /// Where the gathered scrolling happens.
  @ObservationIgnored var wheelAt: DesktopPoint?
  /// Sends the gathered scrolling once the gathering ends.
  @ObservationIgnored var wheelSending: Task<Void, Never>?
  /// The input waiting to go to the helper, in order.
  @ObservationIgnored let input = InputQueue()

  // Reading's working parts (see ViewerController+Reading.swift).

  /// Numbers each start and stop of reading, so a stopped loop ends.
  @ObservationIgnored var generation = 0
  /// Numbers each status reading, so only the newest one started is shown.
  @ObservationIgnored var statusReading = 0
  /// The reading loop.
  @ObservationIgnored var polling: Task<Void, Never>?
  /// The pause between readings, cancelled to read again at once.
  @ObservationIgnored var napping: Task<Void, Never>?
  /// A reading was asked for while one ran: done as soon as it ends.
  @ObservationIgnored var readAgain = false
  /// Refreshes ``now`` while readings run.
  @ObservationIgnored var ticking: Task<Void, Never>?

  /// Creates a viewer that has read nothing yet, with the person's saved rate and sidebar.
  ///
  /// - Parameters:
  ///   - service: The viewer service to read and ask.
  ///   - preferences: Where the rate and the sidebar are kept; a saved rate the picker does not offer reads as every
  ///     2 seconds.
  ///   - clock: The time now, in seconds of uptime.
  ///   - heartbeatInterval: One heartbeat tick; the app passes ``heartbeatEvery``.
  ///   - nap: The pause between readings, which must end early when cancelled; the app passes ``sleep``.
  ///   - logIn: Locks the screen so the person can log in to the helper; nil when the app cannot.
  ///   - showKeys: Offers the keys the Mac keeps for itself, each sent to the controller it is given.
  init(
    service: any ViewerService,
    preferences: ViewerPreferences,
    clock: @escaping @MainActor () -> TimeInterval,
    heartbeatInterval: Duration,
    nap: @escaping @Sendable (Duration) async -> Void,
    logIn: (@MainActor () -> Void)? = nil,
    showKeys: @escaping @MainActor (ViewerController) -> Void
  ) {
    self.service = service
    self.preferences = preferences
    self.clock = clock
    self.heartbeatInterval = heartbeatInterval
    self.nap = nap
    self.logIn = logIn
    self.showKeys = showKeys
    rate = ViewerPictureRate(rawValue: preferences.loadRate()) ?? .everyTwoSeconds
    sidebarShown = preferences.loadSidebar()
    input.sink = self
  }

  /// Everything the controller holds, as one value.
  var snapshot: ViewerSnapshot {
    ViewerSnapshot(
      reading: reading,
      session: session,
      sidebarShown: sidebarShown,
      rate: rate,
      now: now,
      canLogIn: logIn != nil
    )
  }

  /// The window as it should look now, as of the last tick of the clock.
  var state: ViewerState { ViewerPresenter.state(snapshot) }

  /// The header's detail, read again with every tick of the clock: the status line reads it as it draws, so the
  /// picture's age redraws that line alone.
  var headerDetail: String {
    // Read so the status line draws again at each tick.
    _ = ticks
    return ViewerPresenter.detail(snapshot)
  }

  /// Whether this window holds control.
  var holdingControl: Bool { session.holdingControl }

  /// Applies one of the window's events.
  ///
  /// - Parameter event: What the person did.
  func send(_ event: ViewerEvent) {
    switch event {
    case .toggleSidebar: sidebarShown.toggle()
    case .togglePause: togglePause()
    case .takeControl: Task { await takeControl() }
    case .giveBack: releaseControl()
    case .recover: Task { await recover() }
    case .recoverLater: recoverLater()
    case .showRecovery: showRecovery()
    case .showKeys: showKeys(self)
    case .setRate(let rate): self.rate = rate
    case .logIn: logIn?()
    }
  }

  // MARK: Actions

  /// One line about what just happened; it stays until the next.
  ///
  /// - Parameter text: The line, or nil to show none.
  func say(_ text: String?) {
    session.notice = text
  }

  /// Pauses the picture, or resumes it. Either way control goes back; resuming reads afresh.
  func togglePause() {
    session.paused.toggle()
    releaseControl()
    if !session.paused {
      reading.ready = false
      reading.frame = nil
      reading.frameAt = nil
    }
    watch(!session.paused && shown)
  }

  /// Resume reading after the window was hidden, unless paused.
  ///
  /// - Parameter shown: Whether the window is on screen.
  func setVisible(_ shown: Bool) {
    self.shown = shown
    if !shown { releaseControl() }
    watch(shown && !session.paused)
  }

  /// The window entered or left full screen, which hides or shows its title bar buttons.
  ///
  /// - Parameter fullScreen: Whether the window is in full screen.
  func setFullScreen(_ fullScreen: Bool) {
    session.fullScreen = fullScreen
  }

  /// The desktop took the keyboard, or gave it up.
  ///
  /// - Parameter connected: Whether the desktop takes the keyboard now.
  func setKeyboardConnected(_ connected: Bool) {
    session.keyboardConnected = connected
  }

  /// What the input method is composing now, shown on the stage until it is committed or ends. The same text again
  /// changes nothing, so the window does not redraw for it.
  ///
  /// - Parameter text: The composed text; empty once the composition ends.
  func compose(_ text: String) {
    guard session.composing != text else { return }
    session.composing = text
  }

  /// Not Now: sets the recovery alert aside until its reason changes.
  func recoverLater() {
    if let offered = snapshot.offeredRecovery {
      session.recoveryLater = RecoveryKey(offered, status: reading.status)
    }
  }

  /// Recover…: shows the alert set aside again.
  func showRecovery() {
    session.recoveryLater = nil
  }

  /// Recover the helper. Never while holding control; never repeats an action.
  func recover() async {
    guard !session.recovering, !holdingControl else { return }
    session.recovering = true
    session.recoveryLater = nil
    do {
      let restarted = try await service.recover()
      say(restarted ? Words.recovered : Words.readyAgain)
    } catch {
      say(error.localizedDescription)
    }
    session.recovering = false
    // Read the outcome directly: a paused picture makes no requests.
    await readStatus()
    refresh()
  }

  /// The window is closing: give control back and stop reading.
  func close() {
    shown = false
    releaseControl()
    watch(false)
  }

  // MARK: Control

  // `reading`, `session`, `now` and `ticks` are written in this file alone, so the window's shells change them only
  // through the methods above; control and the reading loop in their own files call the ones below.

  /// Take control with the picture on screen. A reply that arrives after
  /// the person changed their mind, or left the window, is given back.
  func takeControl() async {
    guard snapshot.canTakeControl, let frame = reading.frame else { return }
    session.acquiring = true
    controlGeneration += 1
    let generation = controlGeneration
    do {
      let granted = try await service.acquire(frame: frame.id)
      guard generation == controlGeneration, focused, shown, !session.paused, reading.ready else {
        Task { try? await service.release(lease: granted) }
        if generation == controlGeneration { session.acquiring = false }
        return
      }
      session.lease = granted
      session.acquiring = false
      input.activate(granted)
      say(nil)
      startHeartbeat(granted)
      refresh()
    } catch {
      guard generation == controlGeneration else { return }
      session.acquiring = false
      say(error.localizedDescription)
    }
  }

  /// Give control back: at once here, then to the service. Also cancels an
  /// acquisition still under way, whose late grant is then given back.
  ///
  /// - Parameter message: What the notice says, if anything.
  func releaseControl(_ message: String? = nil) {
    controlGeneration += 1
    let generation = controlGeneration
    let previous = session.lease
    session.lease = nil
    session.acquiring = false
    heartbeat?.cancel()
    heartbeat = nil
    clearInput()
    if let previous {
      Task {
        do {
          try await service.release(lease: previous)
          // Show the release at once, not at the next reading: reading the status overtakes a reading under way,
          // so the next one starts at once rather than after the pause.
          if session.lease == nil && generation == controlGeneration {
            await readStatus()
            refresh()
          }
        } catch {
          if session.lease == nil && generation == controlGeneration { say(error.localizedDescription) }
        }
      }
    }
    if let message { say(message) }
  }

  /// Drops every input waiting, the gathered scrolling, the gesture under way and the text being composed, whose
  /// chip would otherwise stay once control ends.
  func clearInput() {
    input.activate(nil)
    session.composing = ""
    wheel.reset()
    gesture.cancel()
    wheelAt = nil
    wheelSending?.cancel()
    wheelSending = nil
  }

  // MARK: Readings

  /// What the panel last saw, shown while the first reading is under
  /// way. It is only a picture: control waits for a reading of its own.
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

  /// Shows a status. Input not ready ends control now; a recovery set aside is forgotten once nothing needs it.
  ///
  /// - Parameter reply: The status read.
  func show(_ reply: ViewerStatus) {
    reading.status = reply
    reading.hasStatus = true
    // Input not ready: control ends now, not with the next picture.
    if !reply.ready {
      reading.ready = false
      if holdingControl || session.acquiring { releaseControl(ViewerWords.inputUnavailable) }
    }
    // Not Now covers one incident: once nothing needs recovery, a later one alerts again.
    if snapshot.offeredRecovery == nil { session.recoveryLater = nil }
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
    reading.frameAt = clock()
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

  /// Reads the clock into ``now``, and tells the status line.
  func readClock() {
    now = clock()
    ticks += 1
  }
}
