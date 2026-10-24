import AtlasService
import AtlasUI
import Foundation
import Observation

/// The alert window's one observed object: which alert shows, whether its command runs, and why it did not run.
/// Its command runs through the helper's shared commands (``HelperActions``), once however many times its button is
/// pressed, after the app's control work is answered; once it worked the window closes, a Recover first saying
/// Recovered for ``AtlasUI/Motion/Duration/doneHold`` on the injected clock, and when it did not, the alert stays and
/// says why. A log-out an app held up asks again, offering Log Out Anyway. Its buttons wait from the
/// press until the status is read again and the outcome shows, and a command closes only the alert it started from
/// (``generation``). ``AlertPresenter`` decides what the alert says.
@Observable
final class AlertController {
  /// Which alert shows.
  private(set) var alert: HelperAlert
  /// Whether its command runs now.
  private(set) var running = false
  /// Why its command did not run; nil until one did not.
  private(set) var failure: String?
  /// Whether a Recover worked and the alert holds, saying so, before it closes.
  private(set) var done = false
  /// An alert asked while a Recover holds on Recovered: it shows once the hold ends, in place of closing.
  @ObservationIgnored private(set) var next: HelperAlert?
  /// Numbers the alert shown: a new alert or a way out moves it on, so a command's ending closes only the alert it
  /// started from.
  @ObservationIgnored private(set) var generation = 0
  /// The helper's own commands, shared with every window.
  @ObservationIgnored let actions: HelperActions
  /// The confirmed command under way, called off by Cancel while it still waits to run.
  @ObservationIgnored private var confirming: Task<Void, Never>?
  /// What the alert window asks of the rest of the app.
  @ObservationIgnored let links: AlertLinks

  /// Creates the alert window's controller.
  ///
  /// - Parameters:
  ///   - alert: Which alert shows.
  ///   - actions: The helper's own commands, shared with every window.
  ///   - links: What the alert window asks of the rest of the app.
  init(alert: HelperAlert, actions: HelperActions, links: AlertLinks) {
    self.alert = alert
    self.actions = actions
    self.links = links
  }

  /// The alert as it should read now.
  var state: AlertCardState<AlertChoice> {
    AlertPresenter.state(
      AlertSnapshot(alert: alert, running: running, failure: failure, done: done, overview: links.overview())
    )
  }

  /// Shows another alert in place of this one. While this one's command runs, the new one is not dropped unseen: the
  /// alert says to try again once its command ends, or, once a Recover holds on Recovered, shows it after the hold.
  ///
  /// - Parameter next: The alert to show.
  func show(_ next: HelperAlert) {
    guard !running else {
      if done {
        self.next = next
      } else {
        failure = next == alert ? failure : HelperWords.waitForAction
      }
      return
    }
    generation += 1
    alert = next
    failure = nil
  }

  /// Applies the button the person pressed.
  ///
  /// - Parameter choice: The button.
  func send(_ choice: AlertChoice) {
    switch choice {
    case .confirm:
      // One press runs it; another before the first has even begun is the same press.
      guard !running, confirming == nil else { return }
      // Unstructured, as `send` is the view's synchronous callback; it clears itself once the command ends.
      confirming = Task {
        await confirm()
        confirming = nil
      }
    case .cancel: cancel()
    }
  }

  /// The way out: closes the window. While its command runs it waits, unless the command is still waiting for this
  /// app's control to be given back: it has not run, so it is called off and the window closes.
  func cancel() {
    if running {
      guard let command = alert.command, actions.isWaiting(command) else { return }
      confirming?.cancel()
    }
    generation += 1
    links.close(self)
  }

  /// The button that goes ahead: its command runs once, whatever else is pressed meanwhile.
  func confirm() async {
    guard !running else { return }
    running = true
    failure = nil
    let started = generation
    switch alert {
    case .recover: await finish(recover(), started: started, refusal: HelperWords.recoverRefused)
    case .switchToHelper: await finish(links.switchToHelper(), started: started)
    case .logOut, .logOutAnyway: await logOut(force: alert == .logOutAnyway, started: started)
    }
  }

  /// Recovers the helper as the main window does. The shared commands reserve it first and read the status afresh
  /// once control is given back, so the choice between starting its input and restarting it follows the helper now.
  ///
  /// - Returns: Its outcome, or nil when a Recover already runs from another window.
  private func recover() async -> Outcome? {
    await actions.recover()?
      .outcome
  }

  /// Logs the helper out, ending control first, unless its mode no longer allows it; one an app held up asks again,
  /// offering Log Out Anyway.
  ///
  /// - Parameters:
  ///   - force: Whether to log out past an app that holds it up.
  ///   - started: The alert's number when its button was pressed.
  private func logOut(force: Bool, started: Int) async {
    let mode = links.overview().mode
    guard force ? mode.allowsForcedLogOut : mode.allowsLogOut else {
      return await finish(Outcome(ok: false, reply: Data(), message: AlertWords.cannotLogOut), started: started)
    }
    let outcome = await actions.logOut(force: force, revoking: mode == .controlling)
    if let outcome, outcome.isLogOutHeld, !force {
      await links.readStatus()
      if started == generation { alert = .logOutAnyway }
      running = false
      return
    }
    await finish(outcome, started: started)
  }

  /// Shows a Recover that worked as Recovered, its buttons still waiting, for ``AtlasUI/Motion/Duration/doneHold``
  /// on the injected clock (v3 A5).
  private func holdDone() async {
    done = true
    // A hold cut short only shortens the moment Recovered shows; the window closes either way.
    try? await links.clock.sleep(for: .seconds(Motion.Duration.doneHold.seconds))
  }

  /// Ends a command: the status is read again with the buttons still waiting, then the window closes once it
  /// worked, a Recover after holding on Recovered, or the alert says why not, and only then can its buttons be
  /// pressed again. Nothing closes or changes when the alert shown is no longer the one the command started from.
  ///
  /// - Parameters:
  ///   - outcome: The command's outcome, or nil when it already ran from another window.
  ///   - started: The alert's number when its button was pressed.
  ///   - refusal: How a refusal reads; its own message by default.
  private func finish(_ outcome: Outcome?, started: Int, refusal: (Outcome) -> String = { $0.message }) async {
    await links.readStatus()
    defer { running = false }
    guard started == generation else { return }
    guard let outcome else {
      failure = HelperWords.waitForAction
      return
    }
    if outcome.ok {
      if alert == .recover { await holdDone() }
      guard started == generation else { return }
      generation += 1
      if let asked = next {
        next = nil
        done = false
        alert = asked
        return
      }
      links.close(self)
    } else {
      failure = Words.sentence(refusal(outcome))
    }
  }
}
