import AtlasKit
import AtlasUI
import Observation

/// The menu bar panel's one observed object: it holds the latest status, the helper's desktop and the last action's
/// outcome, reads them on a schedule, and runs what the panel's events ask. Every action is an `atlas` command or a
/// request to the viewer service; ``PanelPresenter`` decides what the panel shows.
///
/// Each part of the snapshot is its own stored property, so the menu bar glyph, which reads only the status, is not
/// redrawn when a picture arrives.
@MainActor
@Observable
final class PanelController {
  /// Between status readings with the panel open, and between pictures while it is open: 2 s, as the viewer's
  /// default rate, so the open panel looks live.
  static let openEvery: Duration = .seconds(2)
  /// Between status readings with the panel closed: 5 s, often enough for the menu bar glyph to follow the helper's
  /// mode, rarely enough to cost nothing.
  static let closedEvery: Duration = .seconds(5)

  /// The latest status the service reported: every field at its safe default, as a service that cannot be reached
  /// reads, before the first reading ends and after one that fails, which the panel shows alike.
  private(set) var status = ViewerStatus()
  /// How the last status reading ended: nil before the first one, false when it failed, true when the service
  /// answered. Only an answer is handed to the viewer as a reading (``answer``).
  private(set) var answered: Bool?
  /// The helper's desktop, when the last capture gave one; the viewer window opens on it.
  private(set) var picture: DesktopPicture?
  /// Why there is no desktop to show.
  private(set) var placeholder = PanelWords.waitingForPicture
  /// What the last action said. Its spinner shows while any action runs (``running``), whichever said it.
  private(set) var feedback: String?
  /// How many actions run now: the command `perform` runs, and revoking control, for the emergency stop, which never
  /// waits for it, and before a log-out. Kept apart from ``feedback``, so a message never ends an action still
  /// running.
  private(set) var running = 0
  /// The helper's short name, once `atlas switch --list` reports it.
  private(set) var helper: String?
  /// Where Open at Login stands, as last read: at launch, each time the panel opens, and after it is set here.
  private(set) var opensAtLogin: OpenAtLogin.State
  /// Where logging out stands.
  private(set) var logOutStage = LogOutStage.idle

  /// Opens the setup window.
  @ObservationIgnored let gettingStarted: @MainActor () -> Void
  /// Opens the viewer window.
  @ObservationIgnored let openViewer: @MainActor () -> Void
  /// Quits Atlas.
  @ObservationIgnored let quit: @MainActor () -> Void
  /// The viewer service it reads and asks.
  @ObservationIgnored let service: any ViewerService
  /// The `atlas` command line its actions run.
  @ObservationIgnored let commands: any CommandRunning
  /// The app's login item.
  @ObservationIgnored let loginItem: OpenAtLogin
  /// The readings under way.
  @ObservationIgnored private var polling: [Task<Void, Never>] = []
  /// Numbers each status reading, so only the newest one started is shown.
  @ObservationIgnored private var statusReading = 0
  /// Numbers each desktop reading, under the same rule.
  @ObservationIgnored private var frameReading = 0

  /// Creates a panel that has read nothing yet.
  ///
  /// - Parameters:
  ///   - service: The viewer service.
  ///   - commands: The `atlas` command line.
  ///   - loginItem: The app's login item.
  ///   - gettingStarted: Opens the setup window.
  ///   - openViewer: Opens the viewer window.
  ///   - quit: Quits Atlas.
  init(
    service: any ViewerService,
    commands: any CommandRunning,
    loginItem: OpenAtLogin,
    gettingStarted: @escaping @MainActor () -> Void,
    openViewer: @escaping @MainActor () -> Void,
    quit: @escaping @MainActor () -> Void
  ) {
    self.service = service
    self.commands = commands
    self.loginItem = loginItem
    self.gettingStarted = gettingStarted
    self.openViewer = openViewer
    self.quit = quit
    opensAtLogin = loginItem.state
  }

  /// Everything the controller holds, as one value.
  var snapshot: PanelSnapshot {
    PanelSnapshot(
      status: status,
      picture: picture,
      placeholder: placeholder,
      feedback: feedback,
      working: working,
      helper: helper,
      opensAtLogin: opensAtLogin,
      logOutStage: logOutStage
    )
  }

  /// The service's last answer, which the viewer opens on; nil before the first reading ends and after one that
  /// failed.
  var answer: ViewerStatus? { answered == true ? status : nil }

  /// The panel as it should look now.
  var state: PanelState { PanelPresenter.state(snapshot) }

  /// The helper's mode, as the menu bar glyph shows it.
  var mode: Mode { Mode.of(status.overview.mode) }

  /// Whether an action still runs, so every button that starts one waits.
  var working: Bool { running > 0 }

  // MARK: Reading

  /// Reads the status now and then at the open or closed rate, and the desktop at the open rate while open, until
  /// called again. The two run apart, so a slow capture never holds the status back. Opening also reads Open at
  /// Login again, since Getting Started or the system may have changed it meanwhile, and looks for a helper not found
  /// yet.
  ///
  /// - Parameter open: Whether the panel is open.
  func watch(open: Bool) {
    stop()
    if open { readOpenAtLogin() }
    if open, helper == nil { Task { await findHelper() } }
    polling.append(repeating(every: open ? Self.openEvery : Self.closedEvery) { await $0.readStatus() })
    if open { polling.append(repeating(every: Self.openEvery) { await $0.readFrame() }) }
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
    Task { [weak self] in
      while !Task.isCancelled {
        if let controller = self { await step(controller) } else { return }
        try? await Task.sleep(for: pause)
      }
    }
  }

  /// Reads where Open at Login stands now: the panel shows it, and its switch turns it from there.
  func readOpenAtLogin() {
    opensAtLogin = loginItem.state
  }

  /// Learns the helper to switch to: whichever account `atlas` reports.
  func findHelper() async {
    helper = await commands.run(.listAccounts)
      .decode(AccountList.self)?
      .helper
  }

  /// Reads the status. A reading that was cancelled, or overtaken by a newer one, shows nothing.
  func readStatus() async {
    statusReading += 1
    let number = statusReading
    let latest = try? await service.status()
    guard number == statusReading, !Task.isCancelled else { return }
    status = latest ?? ViewerStatus()
    answered = latest != nil
    // A log-out asked for, or offered again, only while it still may run.
    if !working {
      logOutStage = logOutStage.kept(in: status.overview.mode)
    }
  }

  /// Reads the desktop, under the same rule.
  func readFrame() async {
    frameReading += 1
    let number = frameReading
    let shown = try? await service.frame()
    guard number == frameReading, !Task.isCancelled else { return }
    // A failed capture never leaves an old desktop on show.
    picture = shown.flatMap { DesktopPicture(frame: $0) }
    placeholder = PanelWords.pictureUnavailable
  }

  // MARK: Actions

  // Every stored property is written in this file alone, so the panel's shell changes them only through `send`.

  /// Applies one of the panel's events.
  ///
  /// - Parameter event: What the person did.
  func send(_ event: PanelEvent) {
    switch event {
    case .openViewer: openViewer()
    case .revokeControl: Task { await revokeControl() }
    case .switchToHelper: Task { await switchToHelper() }
    case .logIn: Task { await logIn() }
    case .recover: Task { await recover() }
    case .askToLogOut: askToLogOut()
    case .cancelLogOut: cancelLogOut()
    case .logOut: Task { await logOut() }
    case .forceLogOut: Task { await logOut(force: true) }
    case .setOpenAtLogin(let on): setOpenAtLogin(on)
    case .gettingStarted: gettingStarted()
    case .quit: quit()
    }
  }

  /// Shows what an action says. Whether an action runs, and its spinner shows, is ``running``'s, never the
  /// message's.
  ///
  /// - Parameter text: The words, or nil to show nothing.
  private func say(_ text: String?) {
    feedback = text
  }

  /// Runs `action` unless another runs, saying what it does and how it ended, then reads the status.
  ///
  /// - Parameters:
  ///   - action: The `atlas` command to run.
  ///   - working: What to say while it runs.
  ///   - done: What to say when it succeeds; a failure says its own message.
  /// - Returns: The command's outcome, or nil when nothing ran.
  @discardableResult
  private func perform(_ action: Action, working: String, done: String) async -> Outcome? {
    guard !self.working else { return nil }
    running += 1
    say(working)
    let outcome = await commands.run(action)
    say(outcome.ok ? done : outcome.message)
    running -= 1
    await readStatus()
    return outcome
  }

  /// Ends every viewer's control, counted in ``running`` while the service is asked; a refusal says why.
  ///
  /// - Returns: Whether control ended.
  private func revokeEveryViewer() async -> Bool {
    running += 1
    defer { running -= 1 }
    do {
      try await service.revoke()
      return true
    } catch {
      say(error.localizedDescription)
      return false
    }
  }

  /// The emergency stop: ends every viewer's control and never waits for anything else.
  func revokeControl() async {
    say(PanelWords.releasing)
    if await revokeEveryViewer() { say(PanelWords.released) }
    await readStatus()
  }

  /// Switches this Mac's screen to the helper.
  func switchToHelper() async {
    guard let helper else { return }
    await perform(.switchTo(helper), working: PanelWords.switching, done: PanelWords.switched)
  }

  /// Locks the screen so the person can log in to the helper there, finding the helper first if it is not known.
  func logIn() async {
    if helper == nil { await findHelper() }
    guard let helper else {
      say(PanelWords.helperNotFound)
      return
    }
    await perform(
      .switchTo(helper),
      working: Words.locking,
      done: PanelWords.logInThenSwitchBack(to: status.overview.title ?? helper)
    )
  }

  /// Asks the person to confirm logging the helper out.
  func askToLogOut() {
    say(nil)
    logOutStage = .confirming
  }

  /// Drops the confirmation.
  func cancelLogOut() {
    if logOutStage == .confirming { logOutStage = .idle }
  }

  /// Logs the helper out, ending control first; with `force`, even past an app that holds it up. A log-out that
  /// cannot run says why, and one an app held up offers Log Out Anyway, unless the person has asked again meanwhile.
  ///
  /// - Parameter force: Whether to log out past an app that holds it up.
  func logOut(force: Bool = false) async {
    guard !working else { return say(PanelWords.waitForAction) }
    let mode = status.overview.mode
    guard force ? mode.allowsForcedLogOut : mode.allowsLogOut else { return say(PanelWords.cannotLogOut) }
    logOutStage = .idle
    // Control ends first: nothing of the person's stays held in the helper, and a refusal stops the log-out.
    if mode == .controlling {
      guard await revokeEveryViewer() else { return }
    }
    // An emergency stop that started while control ended still runs: the log-out waits, and says so.
    guard
      let outcome = await perform(
        .logOut(force: force),
        working: PanelWords.loggingOut,
        done: PanelWords.helperLoggedOut
      )
    else { return say(PanelWords.waitForAction) }
    // Only the stage this log-out left moves: a confirmation the person reopened meanwhile stays.
    if outcome.isLogOutHeld, logOutStage == .idle { logOutStage = .held }
  }

  /// Recovers the helper.
  func recover() async {
    await perform(.recover, working: Words.recovering, done: Words.readyAgain)
  }

  /// Turns Open at Login on or off, and shows what the system then reports.
  ///
  /// - Parameter on: Whether the app should open at login.
  func setOpenAtLogin(_ on: Bool) {
    do {
      try loginItem.set(on)
    } catch {
      say(error.localizedDescription)
    }
    readOpenAtLogin()
  }
}
