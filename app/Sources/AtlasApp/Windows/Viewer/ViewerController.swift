import AtlasCommands
import AtlasControl
import AtlasService
import AtlasUI
import Foundation
import Observation

/// The main window's one observed object: what the last reading showed, what this window is doing (the tab, watching
/// or paused, holding control, the alert asked), and the person's choices. Pictures stay in memory only. Every effect
/// is a request to the viewer service or one of the helper's commands, which own the helper's safety;
/// ``ViewerPresenter`` decides what the window shows. Its parts are in `ViewerController+Control`, `+Input`,
/// `+InputSink`, `+Reading`, `+Tabs`, `+Picture`, `+Alert`, and for the other tabs `+Panes`, `+Activity`, `+Apps`,
/// `+Shots` and `+Thumbnails`.
@MainActor
@Observable
final class ViewerController {
  /// Between pictures while holding control: 500 ms, so the person sees what their input did at once.
  static let controlEvery: Duration = .milliseconds(500)
  /// How often Details' age of the picture is read again while pictures arrive: 1 s, as it reads in whole seconds.
  static let tickEvery: Duration = .seconds(1)
  /// One heartbeat tick: 500 ms, so a stale picture ends control within half a second; the lease is renewed every
  /// ``renewEvery`` ticks.
  static let heartbeatEvery: Duration = .milliseconds(500)

  /// What the last reading showed. Written only by the controller's own files; the shells call its methods.
  var reading = ViewerReading()
  /// What this window is doing. Written only by the controller's own files; the shells call its methods.
  var session = ViewerSession()
  /// What the Activity, Apps and Screenshots tabs hold. Written only by the controller's own files.
  var panes = ViewerPanes()
  /// Counts the clock's readings. Only ``newestPicture`` reads it, so each tick redraws Details' age row alone.
  private(set) var ticks = 0

  /// How often a picture is read while watching.
  private(set) var rate = ViewerPictureRate.live {
    didSet {
      preferences.saveRate(rate.seconds)
      refresh()
    }
  }
  /// Whether the person asked for Details.
  private(set) var detailsShown = false {
    didSet { preferences.saveDetails(detailsShown) }
  }
  /// The agents connected to Atlas, as last read for Details; nil before they are read, or when they could not be.
  var agents: [AgentID]?
  /// When the helper's account logged in, as last read for Details; nil when unknown.
  var loggedInAt: Date?

  /// The viewer service it reads and asks.
  @ObservationIgnored let service: any ViewerService
  /// The helper's own commands, shared with the panel.
  @ObservationIgnored let actions: HelperActions
  /// The app's control work not yet answered, shared with the helper's commands and any later window.
  @ObservationIgnored var barrier: ControlBarrier { actions.barrier }
  /// Where the rate and Details are kept between windows.
  @ObservationIgnored let preferences: ViewerPreferences
  /// The time now, in seconds of uptime, which never steps back: what a picture's age is told by.
  @ObservationIgnored let uptime: @MainActor () -> TimeInterval
  /// The clock every wait runs on; a cancelled wait ends at once.
  @ObservationIgnored let clock: any Clock<Duration>
  /// One heartbeat tick; the lease is renewed every ``renewEvery`` ticks.
  @ObservationIgnored let heartbeatInterval: Duration
  /// The Mac's double-click speed, in seconds.
  @ObservationIgnored let doubleClickInterval: @MainActor () -> TimeInterval
  /// Where a copied or saved picture goes.
  @ObservationIgnored let pictures: PictureOutlet
  /// Locks the screen so the person can log in to the helper; nil when the app cannot.
  @ObservationIgnored let switchToHelper: (@MainActor () async -> String)?
  /// Reads the agents connected to Atlas, for Details.
  @ObservationIgnored let connectedAgents: @MainActor () async -> [AgentID]?
  /// Reads when an account logged in, for Details.
  @ObservationIgnored let loginTime: @MainActor (String) -> Date?
  /// The date and time now, which the tabs tell times and days by.
  @ObservationIgnored let wallClock: @MainActor () -> Date
  /// Where a copied screenshot goes, and how its file is shown.
  @ObservationIgnored let shotOutlet: ShotOutlet
  /// The time Details last read, in seconds of uptime: refreshed each ``tickEvery`` while pictures arrive.
  /// Unobserved: the window redraws with each reading, and Details' age row follows ``ticks``.
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
  /// The status reading under way when the lease was granted: only a later one can say the lease has ended.
  @ObservationIgnored var controlSince = 0
  /// Renews the lease and ends control over a stale picture.
  @ObservationIgnored var heartbeat: Task<Void, Never>?
  /// The mouse button held and the clicks counted.
  @ObservationIgnored var gesture = PointerGesture()
  /// Scrolling gathered and not yet sent.
  @ObservationIgnored var wheel = WheelAccumulator()
  /// Where the gathered scrolling happens.
  @ObservationIgnored var wheelAt: DesktopPoint?
  /// When the last scroll event came, in seconds of uptime; a long gap starts a new gesture.
  @ObservationIgnored var wheelHeardAt: TimeInterval?
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
  /// When input last asked for a reading, in seconds of uptime.
  @ObservationIgnored var inputReadAt: TimeInterval?
  /// Asks for the reading input asked for too soon after the last, once its pause ends.
  @ObservationIgnored var inputReading: Task<Void, Never>?
  /// The other tabs' reading and thumbnails under way (see ViewerController+Panes.swift).
  @ObservationIgnored var paneWork = ViewerPaneWork()

  /// Creates a viewer that has read nothing yet, with the person's saved rate and Details.
  ///
  /// - Parameter dependencies: What it reaches outside itself; a saved rate the window does not offer reads as Live.
  init(dependencies: ViewerDependencies) {
    service = dependencies.service
    actions = dependencies.actions
    preferences = dependencies.preferences
    uptime = dependencies.uptime
    clock = dependencies.clock
    heartbeatInterval = dependencies.heartbeatInterval
    doubleClickInterval = dependencies.doubleClickInterval
    pictures = dependencies.pictures
    switchToHelper = dependencies.switchToHelper
    connectedAgents = dependencies.connectedAgents
    loginTime = dependencies.loginTime
    wallClock = dependencies.wallClock
    shotOutlet = dependencies.shotOutlet
    rate = ViewerPictureRate(rawValue: preferences.loadRate()) ?? .live
    detailsShown = preferences.loadDetails()
    input.sink = self
  }

  /// Everything the controller holds, as one value.
  var snapshot: ViewerSnapshot {
    ViewerSnapshot(
      reading: reading,
      session: session,
      detailsShown: detailsShown,
      rate: rate,
      now: now,
      running: actions.running,
      agentRequests: actions.agentRequests,
      controlBusy: actions.barrier.isBusy,
      agents: agents,
      loggedInAt: loggedInAt,
      panes: panes,
      date: wallClock()
    )
  }

  /// The window as it should look now, as of the last tick of the clock.
  var state: ViewerState { ViewerPresenter.state(snapshot) }

  /// The ⋯ menu as it should read now.
  var moreMenu: ViewerMoreMenu { ViewerPresenter.moreMenu(snapshot) }

  /// How old the newest picture is, read again with every tick of the clock: Details' row reads it as it draws, so
  /// the picture's age redraws that row alone.
  var newestPicture: String {
    // Read so the row draws again at each tick.
    _ = ticks
    return ViewerPresenter.newest(snapshot)
  }

  /// Whether this window holds control.
  var holdingControl: Bool { session.holdingControl }

  /// Applies one of the window's events. Keys and ⋯ open their menus in the window's shell, which owns AppKit.
  ///
  /// - Parameter event: What the person did.
  func send(_ event: ViewerEvent) {
    switch event {
    case .selectTab(let tab): selectTab(tab)
    case .togglePause: togglePause()
    case .takeControl: Task { await takeControlOnScreen() }
    case .giveBack: releaseControl()
    case .stopAgent: Task { await setStopped(true) }
    case .resumeAgent: Task { await setStopped(false) }
    case .showKeys, .showMore: break
    case .toggleDetails: toggleDetails()
    case .checkAgain: Task { await checkAgain() }
    case .askRecover: ask(.recover)
    case .askSwitch: ask(.switchToHelper)
    case .askLogOut: ask(.logOut)
    case .askRestart: ask(.restart)
    case .alertConfirm: Task { await confirmAlert() }
    case .alertCancel: cancelAlert()
    case .searchActivity, .filterActivity, .searchApps, .showWindow, .clearOutline, .searchShots, .selectShot,
      .openShot, .toggleShot, .previousShot, .nextShot, .closeShot, .copyShot, .showShotInFinder, .thumbnailNeeded:
      sendPane(event)
    }
  }

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

  /// Picks how often a picture is read while watching.
  ///
  /// - Parameter rate: The rate.
  func setRate(_ rate: ViewerPictureRate) {
    self.rate = rate
  }

  /// Shows Details, reading what it lists that the helper's status does not carry, or hides it.
  func toggleDetails() {
    detailsShown.toggle()
    if detailsShown { Task { await readDetails() } }
  }

  /// Resume reading after the window was hidden, unless paused; Details left open reads its facts the first time.
  ///
  /// - Parameter shown: Whether the window is on screen.
  func setVisible(_ shown: Bool) {
    if shown, !self.shown, detailsShown, agents == nil { Task { await readDetails() } }
    self.shown = shown
    if !shown { releaseControl() }
    watch(shown && !session.paused)
    followPane()
  }

  /// The window entered or left full screen, which hides or shows its title bar buttons.
  ///
  /// - Parameter fullScreen: Whether the window is in full screen.
  func setFullScreen(_ fullScreen: Bool) {
    session.fullScreen = fullScreen
  }

  /// The top row's menu that is open now, which its button shows pressed.
  ///
  /// - Parameter menu: The menu open, or nil once it closed.
  func setOpenMenu(_ menu: ViewerMenu?) {
    session.openMenu = menu
  }

  /// The screen took the keyboard, or gave it up.
  ///
  /// - Parameter connected: Whether the screen takes the keyboard now.
  func setKeyboardConnected(_ connected: Bool) {
    session.keyboardConnected = connected
  }

  /// What the input method is composing now, shown on the screen until it is committed or ends. The same text again
  /// changes nothing, so the window does not redraw for it.
  ///
  /// - Parameter text: The composed text; empty once the composition ends.
  func compose(_ text: String) {
    guard session.composing != text else { return }
    session.composing = text
  }

  /// The window is closing: give control back and stop reading.
  func close() {
    shown = false
    releaseControl()
    watch(false)
    followPane()
  }

  /// Reads the clock into ``now``, and tells Details' age row.
  func readClock() {
    now = uptime()
    ticks += 1
  }
}
