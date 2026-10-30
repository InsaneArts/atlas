import AtlasCommands
import AtlasService
import Foundation
import Observation

/// The helper's own commands, run the same way from every window: Recover, Restart, Switch to the Helper, and Log
/// Out with or without force. One of each runs at a time, whichever window asked, and each is answered only once its
/// command has ended; Recover and Restart are one operation, since either may restart the helper. Every one first waits for the app's control work to be answered (``ControlBarrier``) and stops,
/// saying why, while a release is still refused; before switching, logging out or restarting it also ends this
/// app's own control. While any of them runs, Take Control waits (``running``). ``AppContainer`` builds the one
/// instance every window shares.
@Observable
final class HelperActions {
  /// The commands, each with its own in-flight guard.
  enum Command: Hashable, Sendable {
    /// Recover or Restart: either may restart the helper, so one reservation holds both, whichever window asked.
    /// Recover restarts while an outcome is unknown, else starts the helper's input.
    case recovery
    /// Switch this Mac's screen to the helper.
    case switchToHelper
    /// Log the helper out.
    case logOut
  }

  /// The commands running now, from the moment they are asked until their command ends.
  private(set) var running: Set<Command> = []
  /// The commands still waiting for this app's control to be given back, which have not run yet, so calling one off
  /// is safe (``isWaiting(_:)``).
  private(set) var waiting: Set<Command> = []
  /// Stop and Resume requests sent from any window and not answered yet: Resume waits for them, Stop never does.
  private(set) var agentRequests = 0
  /// Shared with setup and notices so menus cannot offer a lock from an off-console session.
  let console: SessionConsole
  /// The `atlas` command line each command runs.
  @ObservationIgnored private let commands: any CommandRunning
  /// The viewer service, which ends every viewer's control before a log-out from a controlled helper.
  @ObservationIgnored private let service: any ViewerService
  /// The app's control work not yet answered, which every command waits for.
  @ObservationIgnored let barrier: ControlBarrier
  /// Ends this app's own control, if a window holds it; its release joins ``barrier``.
  @ObservationIgnored private let endControl: @MainActor () -> Void
  /// Whether the last Recover's input start was refused over an unknown outcome, so the next one restarts, whatever
  /// a status the service still refuses to give last said.
  @ObservationIgnored private var restartNext = false

  /// Creates the commands.
  ///
  /// - Parameters:
  ///   - console: This process's console session, shared with setup and notices.
  ///   - commands: The `atlas` command line.
  ///   - service: The viewer service.
  ///   - barrier: The app's control work not yet answered.
  ///   - endControl: Ends this app's own control, if a window holds it, its release joining `barrier`.
  init(
    console: SessionConsole = .shared,
    commands: any CommandRunning,
    service: any ViewerService,
    barrier: ControlBarrier,
    endControl: @escaping @MainActor () -> Void = {}
  ) {
    self.commands = commands
    self.service = service
    self.barrier = barrier
    self.console = console
    self.endControl = endControl
  }

  /// Whether a command runs now, from any window: control is not taken meanwhile.
  var isBusy: Bool { !running.isEmpty }

  /// Whether `command` is still waiting for this app's control to be given back, before it runs.
  ///
  /// - Parameter command: The command.
  /// - Returns: Whether it waits.
  func isWaiting(_ command: Command) -> Bool {
    waiting.contains(command)
  }

  /// Recovers the helper as the viewer service's own Recover does: once this app's control is given back and every
  /// release answered, the status is read afresh, and the helper restarts while an action's outcome is unknown or
  /// under way, else its input starts; a Recover whose input start was refused over an unknown outcome makes the next
  /// one a restart, whatever a status the service still refuses to give says.
  ///
  /// - Returns: The recovery run and its outcome, a refused release included, or nil when a Recover or a restart
  ///   already runs.
  func recover() async -> HelperRecovery? {
    guard begin(.recovery) else { return nil }
    defer { running.remove(.recovery) }
    if let refused = await freeHelper(.recovery) {
      return HelperRecovery(action: .startInput, outcome: Self.refusal(refused))
    }
    let overview = (try? await service.status())?.overview ?? ViewerStatus.Overview()
    let action = restartNext ? .restart : Action.recover(overview)
    let outcome = await commands.run(action)
    restartNext = outcome.code == "uncertain" && action == .startInput
    return HelperRecovery(action: action, outcome: outcome)
  }

  /// Restarts the helper, ending this app's control first.
  ///
  /// - Returns: Its outcome, a refused release included, or nil when a Restart or a Recover already runs.
  func restart() async -> Outcome? {
    guard begin(.recovery) else { return nil }
    defer { running.remove(.recovery) }
    if let refused = await freeHelper(.recovery) { return Self.refusal(refused) }
    let outcome = await commands.run(.restart)
    // A restart settles what an unknown outcome left, so the next Recover need not restart again.
    if outcome.ok { restartNext = false }
    return outcome
  }

  /// Switches this Mac's screen to `helper`, ending this app's control first.
  ///
  /// - Parameter helper: The helper's account.
  /// - Returns: Its outcome, a refused release included, or nil when a switch already runs.
  func switchTo(_ helper: String) async -> Outcome? {
    guard console.refresh() else { return Self.refusal(SessionConsole.refusal) }
    guard begin(.switchToHelper) else { return nil }
    defer { running.remove(.switchToHelper) }
    if let refused = await freeHelper(.switchToHelper) { return Self.refusal(refused) }
    guard console.refresh() else { return Self.refusal(SessionConsole.refusal) }
    return await commands.run(.switchTo(helper))
  }

  /// Logs the helper out, ending this app's control first and, from a controlled helper, every viewer's; with
  /// `force`, even past an app that holds it up.
  ///
  /// - Parameters:
  ///   - force: Whether to log out past an app that holds it up.
  ///   - revoking: Whether a viewer holds control now, so every viewer's control ends before the log-out.
  /// - Returns: Its outcome, a refused release or revoke included, or nil when a log-out already runs.
  func logOut(force: Bool, revoking: Bool) async -> Outcome? {
    guard begin(.logOut) else { return nil }
    defer { running.remove(.logOut) }
    if let refused = await freeHelper(.logOut) { return Self.refusal(refused) }
    if revoking {
      do {
        try await service.revoke()
      } catch {
        return Self.refusal(error.localizedDescription)
      }
    }
    return await commands.run(.logOut(force: force))
  }

  /// Stops agents, ending an agent's input under way, or lets them go on, counted from any window until the service
  /// answers. Stop never waits; Resume waits while a Stop or Resume is still unanswered, so a late Stop never undoes
  /// it.
  ///
  /// - Parameter stopped: Whether agents stop.
  /// - Returns: What the Stop came to; for Resume, nothing left.
  /// - Throws: ``ServiceError`` when the service refuses, or, for Resume, while another request is unanswered.
  @discardableResult
  func setStopped(_ stopped: Bool) async throws -> StopOutcome {
    guard stopped || agentRequests == 0 else { throw ServiceError(HelperWords.waitForAction) }
    agentRequests += 1
    defer { agentRequests -= 1 }
    return try await service.setStopped(stopped)
  }

  /// Ends this app's control and waits until every acquisition and release is answered, `command` marked waiting
  /// meanwhile; a command called off while it waits never runs.
  ///
  /// - Parameter command: The command that waits.
  /// - Returns: Why the helper is not free yet, the wait's cancellation included, or nil once nothing of this app's
  ///   control is left.
  private func freeHelper(_ command: Command) async -> String? {
    endControl()
    waiting.insert(command)
    defer { waiting.remove(command) }
    return await barrier.settle()
  }

  /// Marks `command` running, unless it already runs.
  ///
  /// - Parameter command: The command.
  /// - Returns: Whether it may run now.
  private func begin(_ command: Command) -> Bool {
    running.insert(command).inserted
  }

  /// A command that never ran, saying why.
  ///
  /// - Parameter message: Why.
  /// - Returns: The failed outcome.
  private static func refusal(_ message: String) -> Outcome {
    Outcome(ok: false, reply: Data(), message: message)
  }
}
