import AtlasService
import AtlasUI
import Observation

/// The main window's one observed object: what this window is doing (the tab, watching or paused, the alert asked)
/// and Details, composed with its three parts, each with state of its own: ``control`` (the lease, its heartbeat and
/// the input that belongs to it), ``readingLoop`` (the status and the picture) and ``panesModel`` (the Activity, Apps
/// and Screenshots tabs). Pictures stay in memory only. Every effect is a request to the viewer service or one of the
/// helper's commands, which own the helper's safety; ``ViewerPresenter`` decides what the window shows from
/// ``snapshot``. Its own parts are in `ViewerController+Tabs`, `+Picture`, `+Alert` and `+Details`.
@Observable
final class ViewerController {
  /// What this window is doing. Written only by the controller's own files; the shells call its methods.
  var session = ViewerSession()
  /// Whether the person asked for Details, and what it read. Written only by the controller's own files; whether it
  /// shows is saved with each change.
  var details = ViewerDetailsFacts() {
    didSet {
      if details.shown != oldValue.shown { dependencies.preferences.saveDetails(details.shown) }
    }
  }
  /// The window is on screen.
  private(set) var shown = false
  /// What it reaches outside itself, which its parts are made from.
  @ObservationIgnored let dependencies: ViewerDependencies
  /// The newest Details read started; older replies cannot replace its facts (`ViewerController+Details`).
  @ObservationIgnored var detailsGeneration = 0
  /// The moment a Recover that worked holds before its alert closes, which Done ends at once.
  @ObservationIgnored let doneHold = DoneHold()
  /// The lease, its heartbeat and release, and the input that belongs to it.
  @ObservationIgnored private(set) lazy var control = ViewerControl(viewer: self, dependencies: dependencies)
  /// The status and the picture, read one at a time while the window shows, and the picture's rate.
  @ObservationIgnored private(set) lazy var readingLoop = ViewerReadingLoop(viewer: self, dependencies: dependencies)
  /// The Activity, Apps and Screenshots tabs, and the thumbnails they share.
  @ObservationIgnored private(set) lazy var panesModel = ViewerPanesModel(viewer: self, dependencies: dependencies) {
    [weak self] list in
    await self?.sessionsRead(list)
  }
  /// Waits for a state that asks something of the person to hold before VoiceOver says it.
  @ObservationIgnored private var announcing: Task<Void, Never>?

  /// The asking text already scheduled or spoken; unchanged readings never restart its hold.
  @ObservationIgnored private var announcedStatus: String?

  /// 1 s: how long a state that asks something of the person holds before VoiceOver says it, so a state that flickers
  /// says nothing and two in a row say only the last.
  static let announceAfter: Duration = .seconds(1)

  /// Creates a viewer that has read nothing yet, with the person's saved rate and Details.
  ///
  /// - Parameter dependencies: What it reaches outside itself; a saved rate the window does not offer reads as Live.
  init(dependencies: ViewerDependencies) {
    self.dependencies = dependencies
    // Made now, so the saved choices are read as the window is made: the rate, then Details.
    _ = readingLoop
    details.shown = dependencies.preferences.loadDetails()
    session.corner =
      dependencies.preferences
      .loadCorner()
      .flatMap(MiniCorner.init(rawValue:)) ?? .bottomTrailing
  }

  /// Everything the controller holds, as one value.
  var snapshot: ViewerSnapshot {
    ViewerSnapshot(
      reading: reading,
      session: session,
      lease: control.lease,
      acquiring: control.acquiring,
      composing: control.composing,
      detailsShown: details.shown,
      rate: readingLoop.rate,
      now: readingLoop.now,
      running: actions.running,
      agentRequests: actions.agentRequests,
      controlBusy: actions.barrier.isBusy,
      agents: details.agents,
      loggedInAt: details.loggedInAt,
      panes: panesModel.panes,
      date: dependencies.wallClock()
    )
  }

  /// The window as it should look now, as of the last tick of the clock.
  var state: ViewerState { ViewerPresenter.state(snapshot) }

  /// The ⋯ menu as it should read now.
  var moreMenu: ViewerMoreMenu { ViewerPresenter.moreMenu(snapshot) }

  /// The sessions menu as it should read now.
  var sessionsMenu: ViewerSessionsMenu { ViewerPresenter.sessionsMenu(snapshot) }

  /// How old the newest picture is, read again with every tick of the clock: Details' row reads it as it draws, so
  /// the picture's age redraws that row alone.
  var newestPicture: String {
    // Read so the row draws again at each tick.
    _ = readingLoop.ticks
    return ViewerPresenter.newest(snapshot)
  }

  /// The viewer service it reads and asks.
  var service: any ViewerService { dependencies.service }

  /// The helper's own commands, shared with the panel.
  var actions: HelperActions { dependencies.actions }

  /// The app's control work not yet answered, shared with the helper's commands and any later window.
  var barrier: ControlBarrier { actions.barrier }

  /// What the last reading showed.
  var reading: ViewerReading { readingLoop.reading }

  /// What the Activity, Apps and Screenshots tabs hold.
  var panes: ViewerPanes { panesModel.panes }

  /// Whether this window holds control.
  var holdingControl: Bool { control.lease != nil }

  /// Applies one of the window's events. Keys, ⋯ and the sessions button open their menus in the window's shell, which
  /// owns AppKit.
  ///
  /// - Parameter event: What the person did.
  func send(_ event: ViewerEvent) {
    // Unstructured, as `send` is the view's synchronous callback: each async event runs in a task of its own.
    switch event {
    case .selectTab(let tab): selectTab(tab)
    case .togglePause: togglePause()
    case .takeControl: Task { await control.takeControlOnScreen() }
    case .giveBack: control.releaseControl()
    case .stopAgent: Task { await setStopped(true) }
    case .resumeAgent: Task { await setStopped(false) }
    case .showKeys, .showMore, .showSessions, .placeSessions: break
    case .chooseSession(let choice): chooseSession(choice)
    case .toggleDetails: toggleDetails()
    case .checkAgain: Task { await checkAgain() }
    case .openSettings: dependencies.openSettings()
    case .askRecover: ask(.recover)
    case .askSwitch: ask(.switchToHelper)
    case .askLogOut: ask(.logOut)
    case .askRestart: ask(.restart)
    case .askCleanup(let key): Task { await askCleanup(key) }
    case .alertConfirm: Task { await confirmAlert() }
    case .alertCancel: cancelAlert()
    case .moveCorner(let corner): moveCorner(corner)
    case .searchActivity, .filterActivity, .searchApps, .showWindow, .clearOutline, .shotColumns, .selectShot,
      .openShot, .toggleShot, .previousShot, .nextShot, .shotAbove, .shotBelow, .clearShotSelection, .closeShot,
      .copyShot, .showShotInFinder, .thumbnailNeeded:
      panesModel.sendPane(event)
    }
  }

  /// Take Control or Give Back from a menu, presenting the window before Take Control so a paused picture can resume.
  ///
  /// - Parameter present: Brings the window forward and updates its visibility and focus.
  func toggleControl(present: () -> Void) {
    guard let control = ViewerPresenter.actions(snapshot).control, control.isEnabled,
      control.event == .takeControl || control.event == .giveBack
    else { return }
    if control.event == .takeControl { present() }
    send(control.event)
  }

  /// The small screen settled in a corner: the window keeps it there, and the next window starts there.
  ///
  /// - Parameter corner: The corner.
  func moveCorner(_ corner: MiniCorner) {
    session.corner = corner
    dependencies.preferences.saveCorner(corner.rawValue)
  }

  /// What VoiceOver should say about the helper's state now: its word and line while it asks something of the person
  /// (it needs them, agents are stopped, they have control, it is logged out or not ready) and this window has the
  /// keyboard; nil otherwise, as for every Working and Idle.
  var askingStatus: String? {
    guard shown, control.focused else { return nil }
    let snapshot = snapshot
    switch snapshot.mode.kind {
    case .needsYou, .stopped, .control, .loggedOut, .notReady:
      let status = ViewerPresenter.status(snapshot)
      return status.line.isEmpty ? status.label : "\(status.label), \(status.line)"
    case .idle, .working, .inUseElsewhere, .connecting:
      return nil
    }
  }

  /// Says `text` once it has held for ``announceAfter`` on the window's clock; a newer one, or none, calls it off.
  ///
  /// - Parameter text: What VoiceOver should say now, from ``askingStatus``.
  func announceSoon(_ text: String?) {
    guard text != announcedStatus else { return }
    announcedStatus = text
    announcing?.cancel()
    announcing = nil
    guard let text else {
      if session.announcement != nil { session.announcement = nil }
      return
    }
    let clock = dependencies.clock
    // Unstructured, as the state settles while the window goes on.
    announcing = Task { [weak self] in
      try? await clock.sleep(for: Self.announceAfter)
      guard let self, !Task.isCancelled else { return }
      session.announcement = text
    }
  }

  /// One line about what just happened; it stays until the next.
  ///
  /// - Parameter text: The line, or nil to show none.
  func say(_ text: String?) {
    session.notice = text
  }

  /// Take Control was refused: the reason shows in a toast and the button shakes once. The toast leaves by
  /// itself after ``Motion/Duration/toast`` on the window's clock; only the newest refusal's wait removes it.
  ///
  /// - Parameter reason: Why, such as "The agent is mid-action. Try again in a moment."
  func refuse(_ reason: String) {
    session.refusals += 1
    session.toast = reason
    let refusal = session.refusals
    let clock = dependencies.clock
    // Unstructured, as the toast leaves by itself while the window goes on.
    Task { [weak self] in
      try? await clock.sleep(for: .seconds(Motion.Duration.toast.seconds))
      guard let self, session.refusals == refusal else { return }
      session.toast = nil
    }
  }

  /// The toast goes at once: control was taken after all.
  func dismissToast() {
    if session.toast != nil { session.toast = nil }
  }

  /// Stop or Resume, saying how it went, then reads the status so the window shows it at once.
  ///
  /// - Parameter stopped: Whether agents stop.
  func setStopped(_ stopped: Bool) async {
    do {
      let outcome = try await actions.setStopped(stopped)
      say(stopped ? HelperWords.stopped(outcome) : ViewerWords.agentResumed)
    } catch {
      say(error.localizedDescription)
    }
    await readingLoop.readStatus()
    readingLoop.refresh()
  }

  /// Pauses the picture, or resumes it. Either way control goes back; resuming reads afresh.
  func togglePause() {
    session.paused.toggle()
    control.releaseControl()
    if !session.paused { readingLoop.forgetFrame() }
    readingLoop.watch(shown)
  }

  /// Shows Details, reading what it lists that the helper's status does not carry, or hides it.
  func toggleDetails() {
    details.shown.toggle()
    // Unstructured, as the event is synchronous: Details reads its facts beside it.
    if details.shown { Task { await readDetails() } }
  }

  /// Resume reading after the window was hidden; Details left open reads its facts the first time.
  ///
  /// - Parameter shown: Whether the window is on screen.
  func setVisible(_ shown: Bool) {
    // Unstructured, as AppKit's notification is synchronous: Details reads its facts beside it.
    if shown, !self.shown, details.shown, details.agents == nil { Task { await readDetails() } }
    self.shown = shown
    if !shown { control.releaseControl() }
    readingLoop.watch(shown)
    panesModel.followPane()
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
    // The sessions menu asked for has opened, once.
    if menu == .sessions { session.sessionsMenuWanted = false }
  }

  /// Shows every host session's work, or one's, in Activity and Screenshots and at the top of Details; the list of
  /// sessions is read again for it. A screenshot shown large goes, as it may no longer be among those listed.
  ///
  /// - Parameter choice: Whose work.
  func chooseSession(_ choice: ViewerSessionChoice) {
    guard choice != session.chosenSession else { return }
    session.chosenSession = choice
    panesModel.closeShot()
    panesModel.followSessions(readNow: true)
  }

  /// Asks for the sessions menu to open once its button shows, as the panel's row for more sessions does.
  func askForSessionsMenu() {
    session.sessionsMenuWanted = true
  }

  /// The screen took the keyboard, or gave it up.
  ///
  /// - Parameter connected: Whether the screen takes the keyboard now.
  func setKeyboardConnected(_ connected: Bool) {
    session.keyboardConnected = connected
  }

  /// The window is closing: give control back and stop reading; a sessions menu asked for is forgotten.
  func close() {
    shown = false
    session.sessionsMenuWanted = false
    control.releaseControl()
    readingLoop.watch(false)
    panesModel.followPane()
    panesModel.stopCopy()
  }
}
