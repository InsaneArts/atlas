import AtlasCommands
import AtlasOnboarding
import AtlasService
import AtlasUI
import Foundation
import Observation

/// The setup window's one observed object: the facts `atlas onboarding status` reports (read again every moment
/// while the window is open), the person's choices, and what is running now. Every action is an `atlas` command; the
/// window holds no policy of its own, and ``OnboardingPresenter`` decides what it shows.
///
/// ``run()`` holds the window's two lanes: the reading loop (`+Reading`) and the queue (`+Queue`), which performs the
/// window's requests one at a time. ``send(_:)`` applies a choice at once and queues an action as an
/// ``OnboardingRequest``. The rest is in its parts: the account (`+Account`), the check and Done (`+Finishing`), the
/// agents (`+Agents`) and the helper's own screens (`+Permissions`).
///
/// What the window shows is written by the controller's own files alone, so the window's shell and view change it only
/// through ``send(_:)`` and ``update(_:)``.
@Observable
final class OnboardingController {
  /// How often the facts are read while the window is open: 1.5 s, soon enough that a switch turned on in System
  /// Settings, or a log-in in the helper, shows without the person waiting.
  static let readEvery: Duration = .seconds(1.5)
  /// After `atlas agents status` failed, how many readings of the facts pass before it is asked again: 10, about 15
  /// s, so a command that keeps failing is not run every moment while the person reads the screen.
  static let agentsRetryReadings = 10
  /// How many readings in a row a job may answer nothing before it counts as stopped: 20, about 30 s, longer than
  /// launchd waits to bring a job back.
  static let restartReadings = 20
  /// How long a permission just turned on shows its tick before the next one is allowed: 1.2 s.
  static let tickShown: Duration = .milliseconds(1200)
  /// How long the passed check shows before its Continue lights up and takes presses: ``Motion/Duration/followUp``.
  static let continueFollows: Duration = .seconds(Motion.Duration.followUp.seconds)
  /// How long a command runs before its working line shows: ``Motion/Duration/noteDelay``, the line's own wait, so a
  /// command that ends sooner never shows one.
  static let workingShows: Duration = .seconds(Motion.Duration.noteDelay.seconds)
  /// The least time a working line stays once it shows: ``Motion/Duration/leastWork``, so a line never flashes.
  static let workingStays: Duration = .seconds(Motion.Duration.leastWork.seconds)

  /// Everything the window shows, as one value.
  var snapshot = OnboardingSnapshot()
  /// The window's work is done: setup finished, or the helper locked for the switch back. The window closes itself.
  var closeRequested = false

  /// What the controller reaches outside itself.
  @ObservationIgnored let dependencies: OnboardingDependencies
  /// The tickets of the queued requests, in order, for the queue in ``run()``.
  @ObservationIgnored let requests: AsyncStream<Int>
  /// Where the window queues a request's ticket.
  @ObservationIgnored let queue: AsyncStream<Int>.Continuation
  /// The requests queued and not yet started, by their tickets, so one already waiting is not queued twice, and Back
  /// can drop one. Each queuing takes a ticket of its own, so a request dropped and queued again runs in its new place,
  /// never its old one.
  @ObservationIgnored var pending: [Int: OnboardingRequest] = [:]
  /// The ticket the last queued request took.
  @ObservationIgnored var lastTicket = 0
  /// The request the queue performs now, with its work, which Back can stop; nil between requests. Read by tests,
  /// which wait for its end.
  @ObservationIgnored var performing: (request: OnboardingRequest, work: Task<Void, Never>)?
  /// Whether ``run()`` has started, so a second call returns at once.
  @ObservationIgnored var started = false
  /// Whether the reading loop is setting the helper's mark now.
  @ObservationIgnored var marking = false
  /// The window's close, waiting for the mark under way to settle, so the clearing lands last.
  @ObservationIgnored var markSettled: CheckedContinuation<Void, Never>?
  /// The reading loop's pause between readings, which a refresh or the window's close cuts short.
  @ObservationIgnored var resting: Task<Void, Never>?
  /// A reading asked for since the last one began: done as soon as it ends.
  @ObservationIgnored var readAgain = false
  /// Whether the helper's "set up here" notice was last told the window is open.
  @ObservationIgnored var markedOpen = false
  /// Readings of the facts since `atlas agents status` last failed.
  @ObservationIgnored var readingsSinceAgentsFailed = 0
  /// Readings in a row whose permissions were unknown.
  @ObservationIgnored var readingsUnknown = 0
  /// In the helper: the account whose picture was read last, by its short name, so it is read once.
  @ObservationIgnored var askerPictured: String?
  /// The window closed: nothing new starts, nothing queued runs, and late replies are dropped.
  @ObservationIgnored var closed = false
  /// The time since Lock and Switch, while the helper's login is timed; nil otherwise.
  @ObservationIgnored var sinceSwitch: (@Sendable () -> Duration)?
  /// Told each time the queue takes a request, once it is the request under way and before its work starts, which
  /// waits for the turn of the main actor the telling runs in to end: Back or the window's close can come in that gap,
  /// and a test acts there.
  @ObservationIgnored var taking: (@MainActor (OnboardingRequest) -> Void)?
  /// The steps of the helper's login noted so far: "answering" and "hidden".
  @ObservationIgnored var timed: Set<String> = []

  /// Creates a controller that has read nothing yet.
  ///
  /// - Parameter dependencies: The `atlas` command line and the system pieces it reaches.
  init(dependencies: OnboardingDependencies) {
    self.dependencies = dependencies
    let (requests, queue) = AsyncStream.makeStream(of: Int.self)
    self.requests = requests
    self.queue = queue
    snapshot.agentIcons = dependencies.agentIcons()
  }

  /// The `atlas` command line every action runs.
  var commands: any CommandRunning { dependencies.commands }

  /// The window as it should look now, with whether an agents install runs from any window.
  var state: OnboardingState {
    var shown = snapshot
    shown.isOnConsole = dependencies.console.isOnConsole
    shown.agentsInstalling = dependencies.installs.isBusy
    return OnboardingPresenter.state(shown)
  }

  /// Applies one of the window's events: a choice at once, an action as a request in the queue.
  ///
  /// - Parameter event: What the person did.
  func send(_ event: OnboardingEvent) {
    switch event {
    case .begin: snapshot.choices.started = true
    case .back: back()
    case .pick(let account): snapshot.picked = account
    case .continueChoice: enqueue(.useHelper)
    case .createAccount: enqueue(.createAccount)
    case .lockAndSwitch: enqueue(.lockAndSwitch)
    case .chooseAnother: enqueue(.chooseAnother)
    case .closeLeftovers: enqueue(.closeLeftovers)
    case .continueAfterClose: snapshot.choices.closedLeftovers = false
    case .checkAgain, .continueFinishing: refresh()
    case .retryFinish: retryFinish()
    case .setAgent(let id, let on): setAgent(id, on: on)
    case .connectAgents: enqueue(.connectAgents(chosen: agentsSwitchedOn))
    case .setOpenAtLogin(let on):
      guard snapshot.working == nil else { return }
      snapshot.opensAtLogin = on
    case .complete: enqueue(.complete)
    case .allow:
      let asker = snapshot.facts.asker
      enqueue(.allow(uid: asker.uid, name: asker.name))
    // Nothing is allowed: the window closes, and the helper asks again the next time Atlas opens there.
    case .notNow: closeRequested = true
    case .showInFinder: dependencies.reveal(dependencies.app)
    case .openSettings(let id):
      if let permission = Permission.allCases.first(where: { $0.id == id }) { enqueue(.openSettings(permission)) }
    case .closePanel: closePanel()
    case .switchBack: enqueue(.switchBack)
    }
  }

  /// The account form, as the person edits it in place.
  ///
  /// - Parameter form: The form as it now reads.
  func update(_ form: OnboardingAccountForm) {
    snapshot.form = form
  }

  /// Reads the facts now and then every ``readEvery``, and performs the window's requests one at a time, until
  /// ``close()``.
  ///
  /// The window runs this once, from when it is first shown; a second call returns at once, and so does one after the
  /// close. Its two lanes are the two children of one task group, so neither outlives it: the reading loop
  /// (`readFacts()`) and the queue (`performRequests()`). Both run on the main actor and interleave only at their
  /// awaits, so a reading lands while an action runs, and no action waits for a reading.
  ///
  /// It returns once the window has closed and the request under way, if any, has settled. Cancelling it stops both
  /// lanes at once, the request under way included.
  func run() async {
    guard !started, !closed else { return }
    started = true
    // Structured children, not tasks of their own: each runs its whole loop on the main actor.
    await withDiscardingTaskGroup { group in
      group.addTask { await self.readFacts() }
      group.addTask { await self.performRequests() }
    }
  }

  /// The window is closing: the queue stops, so nothing queued runs, and the reading loop stops, dropping a reading
  /// under way; once a mark under way has settled, the helper's mark is cleared, so the clearing lands last. A request
  /// under way runs to its end, since stopping `atlas` partway could leave a restart or setup half done, and its
  /// outcome is dropped.
  func close() async {
    guard !closed else { return }
    closed = true
    queue.finish()
    pending.removeAll()
    resting?.cancel()
    if marking { await withCheckedContinuation { markSettled = $0 } }
    if snapshot.facts.role == .helper {
      _ = await commands.run(.markWindow(open: false))
    }
  }

  /// Runs one command, with its words shown while it runs; one at a time, and none once the window has closed. A
  /// command that ran past ``workingShows`` has shown its line, which stays ``workingStays`` before the next screen or
  /// line takes its place; a problem shows at once.
  ///
  /// A command already running when the window closes settles, but its outcome is dropped, so a workflow awaiting it
  /// stops there and starts no further command. So is one whose wait Back stopped (``OnboardingRequest/endsOnBack``).
  ///
  /// - Parameters:
  ///   - action: The command.
  ///   - words: What to show while it runs.
  ///   - input: What to write to its standard input, if anything: a secret travels this way.
  /// - Returns: The command's outcome, or nil when another one runs, Back stopped it or the window has closed.
  @discardableResult
  func act(_ action: Action, _ words: String, input: Data? = nil) async -> Outcome? {
    switch action {
    case .switchTo, .lockScreen:
      guard dependencies.console.refresh() else {
        snapshot.problem = SessionConsole.refusal
        return nil
      }
    default: break
    }
    return await act(words) { await self.commands.run(action, input: input) }
  }

  /// Runs `command` as ``act(_:_:input:)`` runs a command; one that could not start, as an agents install while
  /// another window's runs, says so.
  ///
  /// - Parameters:
  ///   - words: What runs, in words, while it does.
  ///   - command: The command, answering nil when it could not start.
  /// - Returns: Its outcome, or nil when it did not run, Back stopped it or the window closed.
  func act(_ words: String, _ command: () async -> Outcome?) async -> Outcome? {
    guard !closed, snapshot.working == nil else { return nil }
    snapshot.working = words
    snapshot.problem = nil
    snapshot.problemAbout = nil
    let ran = Self.stopwatch(dependencies.clock)
    let outcome = await command()
    if outcome?.ok == true { await keepWorkingLine(shownFor: ran()) }
    snapshot.working = nil
    guard !closed, !Task.isCancelled else { return nil }
    guard let outcome else {
      snapshot.problem = HelperWords.waitForAction
      return nil
    }
    if !outcome.ok { snapshot.problem = Words.sentence(outcome.message) }
    refresh()
    return outcome
  }

  /// Keeps a working line that has shown until it has stayed ``workingStays``; one whose command ended before it
  /// showed goes at once. Cancelled, as by Back or the window's close, it ends at once.
  ///
  /// - Parameter elapsed: How long the command ran.
  private func keepWorkingLine(shownFor elapsed: Duration) async {
    let left = Self.workingShows + Self.workingStays - elapsed
    guard elapsed > Self.workingShows, left > .zero else { return }
    try? await dependencies.clock.sleep(for: left)
  }

  /// Back to the screen before: the welcome from the choice, the choice from the form, the agents from Done. It is
  /// applied at once, never queued, and stops the lock's wait, waiting or under way.
  private func back() {
    stop(where: \.endsOnBack)
    switch snapshot.step {
    case .chooseHelper: snapshot.choices.started = false
    case .createAccount: snapshot.choices.creating = false
    case .done: snapshot.choices.agentsDone = false
    default: break
    }
  }
}
