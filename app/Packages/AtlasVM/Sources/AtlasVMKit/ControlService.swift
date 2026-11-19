import Foundation

/// A VM's runner as its station sees it: each request answered by this
/// run, one change done at a time, each kept as asked before it is done and
/// as settled after, and the changes an earlier run left settled by what is
/// there now.
public actor ControlService {
  private let ledger: ChangeLedger
  private let host: any VMHost
  private let incarnation: Incarnation
  /// Whether a change, or recovery, is being done now.
  private var busy = false
  /// Whether the VM is being removed: from `forget`'s asking, no other
  /// change is taken.
  private var forgetting = false
  /// Whether this run is giving way to a newer build, or looking whether
  /// it may: while it is, no change is taken.
  private var retiring = false
  /// Whether it gave way: for good.
  private var retired = false
  /// How many changes are being done now, reports among them.
  private var inFlight = 0
  /// The lock this run serves under, once it recovered: its proof that
  /// every earlier run has ended.
  private var lock: RunnerLock?
  /// Whether this run settled what earlier runs left, holding its lock.
  private var hasRecovered = false
  /// What tells the station whether the window was opened between two of
  /// its states.
  private var fence = PictureFence()

  /// A runner serving `host` as `incarnation`, its changes kept in `ledger`.
  public init(ledger: ChangeLedger, host: any VMHost, incarnation: Incarnation) {
    self.ledger = ledger
    self.host = host
    self.incarnation = incarnation
  }

  /// The reply line, newline included, to the request line `line`.
  public func answer(line: Data) async -> Data {
    let reply: ControlReply
    if let operation = try? JSONDecoder().decode(ControlOperation.self, from: line) {
      reply = await answer(operation)
    } else {
      reply = .refused("That is not a request a VM's runner serves.", by: incarnation)
    }
    let encoder = JSONEncoder()
    encoder.outputFormatting = .sortedKeys
    var data = (try? encoder.encode(reply)) ?? Data(#"{"ok":false}"#.utf8)
    data.append(UInt8(ascii: "\n"))
    return data
  }

  /// The reply to `operation`.
  public func answer(_ operation: ControlOperation) async -> ControlReply {
    switch operation {
    case .hello:
      var reply = ControlReply.served("hello", by: incarnation)
      reply.hello = .ours
      return reply
    case .state:
      // Read before the window: an opening begun since moves it on, and one
      // under way or ended meanwhile says the window open.
      let reading = fence
      let power = await host.power()
      var reply = ControlReply.served("state", by: incarnation)
      reply.power = power.value
      reply.confirmed = power.confirmed
      let shown = await host.windowOpen()
      reply.window = shown || fence.mayHaveOpened(since: reading) ? .open : .closed
      reply.fence = reading.value
      // Only a run that recovered holding the lock knows every earlier run
      // ended, so only it can say their VMs, and windows, are gone.
      reply.earlierGone = hasRecovered ? await host.earlierGone() : false
      reply.making = await host.making()
      reply.disk = await host.disk()
      return reply
    case .latest:
      var reply = ControlReply.served("latest", by: incarnation)
      reply.latest = await host.latest()
      return reply
    case .outcome(let change):
      let record: ChangeRecord?
      do {
        record = try ledger.record(change)
      } catch {
        return .refused("What the change came to cannot be read.", by: incarnation)
      }
      var reply = ControlReply.served("outcome", by: incarnation)
      reply.change = change
      guard let record else { return reply }
      return await answered(record, in: reply)
    case .installResume(let install):
      guard !retiring else {
        return .refused("Its runner is being replaced; ask again in a moment.", by: incarnation)
      }
      return await resume(install)
    case .void(let change):
      return await void(change)
    case .create, .start, .stop, .window, .install, .paired, .closed, .forget:
      return await change(operation)
    }
  }

  /// Make sure the change `change` is never done from now. Kept as refused
  /// when it was never asked, in the one step that would have kept it, so a
  /// request of it still on its way is answered as another change asked
  /// with its ID and never done; answered as what it came to when it was.
  /// Nothing of the VM is touched, so it waits for no change being done.
  private func void(_ change: ChangeID) async -> ControlReply {
    var reply = ControlReply.served(ControlOperation.void(change).name, by: incarnation)
    reply.change = change
    let record = ChangeRecord(operation: .void(change), run: incarnation.id)
    do {
      switch try ledger.begin(.void(change), by: incarnation.id) {
      case .new:
        try ledger.settle(record, as: Self.voided)
        reply.by = record.run
        reply.outcome = Self.voided
        reply.pending = false
        return reply
      case .asked(let kept):
        return await answered(kept, in: reply)
      case .conflicting:
        guard let kept = try ledger.record(change) else {
          return .refused("What the change came to cannot be read.", by: incarnation)
        }
        return await answered(kept, in: reply)
      }
    } catch {
      return .refused("The change could not be kept, so nothing was voided.", by: incarnation)
    }
  }

  /// What a change voided before it was asked came to.
  static let voided = ChangeOutcome.refused("Voided before it was asked; it is never done.")

  /// Go on with the install `install`, asked again without its secret: an
  /// install left pending alone, by this run or, once recovered, an earlier
  /// one's. It is no change of its own: what it comes to is the install's.
  private func resume(_ install: ChangeID) async -> ControlReply {
    guard !forgetting else {
      return .refused("This VM is being removed.", by: incarnation)
    }
    var reply = ControlReply.served(ControlOperation.installResume(install).name, by: incarnation)
    reply.change = install
    let kept: ChangeRecord?
    do {
      kept = try ledger.record(install)
    } catch {
      return .refused("What the change came to cannot be read.", by: incarnation)
    }
    guard let kept, case .install = kept.operation else {
      return .refused("No install was asked with this ID.", by: incarnation)
    }
    guard kept.outcome == nil else { return await answered(kept, in: reply) }
    guard kept.run == incarnation.id || lock != nil else {
      return .refused("Its runner has not recovered yet; ask again in a moment.", by: incarnation)
    }
    guard !busy else {
      return .refused("Another change is being done; ask again once it is.", by: incarnation)
    }
    return await go(on: kept, asked: .installResume(install), in: reply)
  }

  /// Give way to a newer build of this runner, as its program was replaced:
  /// only while no change is being done (reports and resumed ones among
  /// them), no removal is under way, its window is closed and the VM is
  /// seen stopped, or not made. From then every change is refused, one
  /// left pending among them, for the next run to answer.
  /// Whether it may go now.
  public func retire() async -> Bool {
    if retired { return true }
    guard !retiring, !busy, !forgetting, inFlight == 0 else { return false }
    // Closed before anything is awaited, so no change, report or removal
    // comes in while it looks; opened again when it may not go.
    retiring = true
    let windowed = await host.windowOpen()
    let power = await host.power()
    guard !windowed, power.confirmed, power.value == .stopped || power.value == .absent, inFlight == 0 else {
      retiring = false
      return false
    }
    retired = true
    return true
  }

  /// Settle each change an earlier run began and left intended, by what is
  /// there now. `lock` proves every earlier run has ended: this run holds
  /// the one lock a run serves under. A window's change is settled only with
  /// `vmGone` too, the VMs those runs ran seen gone, so no window of theirs
  /// can still have input. One that cannot be told stays pending.
  public func recover(holding lock: RunnerLock, vmGone: Bool) async {
    guard !busy else { return }
    self.lock = lock
    busy = true
    defer { busy = false }
    guard let unsettled = try? ledger.unsettled() else { return }
    for record in unsettled where record.run != incarnation.id {
      guard let outcome = await recovered(record.operation, vmGone: vmGone) else { continue }
      try? ledger.settle(record, as: outcome)
    }
    hasRecovered = true
  }

  private func change(_ operation: ControlOperation) async -> ControlReply {
    // Giving way, or looking whether it may: no change of any kind.
    guard !retiring else {
      return .refused("Its runner is being replaced; ask again in a moment.", by: incarnation)
    }
    if case .forget = operation { forgetting = true }
    var reply = ControlReply.served(operation.name, by: incarnation)
    reply.change = operation.change
    let beginning: ChangeBeginning
    do {
      // Asked again, it is answered from its record even while another runs.
      if let change = operation.change, let kept = try ledger.record(change) {
        guard kept.operation == operation.recorded else {
          return .refused("Another change was asked with this ID.", by: incarnation)
        }
        // An install left pending goes on from where it got when asked
        // again, by this run or, once recovered, an earlier one's: its
        // phases are each safe to repeat, and only the asking carries its
        // secret again.
        if operation.resumes, kept.outcome == nil, kept.run == incarnation.id || lock != nil, !busy, !retiring,
          !forgetting || operation.isForget
        {
          return await go(on: kept, asked: operation, in: reply)
        }
        return await answered(kept, in: reply)
      }
      guard !forgetting || operation.isForget else {
        return .refused("This VM is being removed.", by: incarnation)
      }
      guard !retiring else {
        return .refused("Its runner is being replaced; ask again in a moment.", by: incarnation)
      }
      // A report touches only its install's record, so it never waits.
      guard !busy || operation.isReport else {
        return .refused("Another change is being done; ask again once it is.", by: incarnation)
      }
      beginning = try ledger.begin(operation.recorded, by: incarnation.id)
    } catch {
      return .refused("The change could not be kept, so nothing was done.", by: incarnation)
    }
    switch beginning {
    case .conflicting:
      return .refused("Another change was asked with this ID.", by: incarnation)
    case .asked(let record):
      return await answered(record, in: reply)
    case .new:
      return await go(
        on: ChangeRecord(operation: operation.recorded, run: incarnation.id),
        asked: operation,
        in: reply
      )
    }
  }

  /// Do `operation`, kept as `record`, and keep what it came to. A report
  /// does not hold the one-at-a-time guard; a `closed` report settles the
  /// install it answers.
  private func go(on record: ChangeRecord, asked operation: ControlOperation, in reply: ControlReply) async
    -> ControlReply
  {
    var reply = reply
    reply.by = record.run
    let guarded = !operation.isReport
    if guarded { busy = true }
    inFlight += 1
    defer {
      if guarded { busy = false }
      inFlight -= 1
    }
    guard let outcome = await perform(operation) else {
      reply.pending = true
      return reply
    }
    // The install a `closed` report answers is settled first: the report
    // stays intended until it is, and asked again settles it.
    if case .closed(_, let install) = operation, outcome == .done, stillPending(install) {
      guard let installed = try? ledger.record(install), (try? ledger.settle(installed, as: .done)) != nil else {
        reply.pending = true
        return reply
      }
    }
    do {
      try ledger.settle(record, as: outcome)
      reply.outcome = outcome
      reply.pending = false
    } catch {
      // Done or not, it stays intended: an outcome query asks again.
      reply.pending = true
    }
    return reply
  }

  /// `reply`, answering for `record`: one this run left pending, its
  /// effect not seen or not kept then, is looked at again first and settled
  /// if what is there now tells.
  private func answered(_ record: ChangeRecord, in reply: ControlReply) async -> ControlReply {
    var reply = reply
    var record = record
    reply.by = record.run
    if record.outcome == nil, record.run == incarnation.id || lock != nil, !busy, !retiring {
      busy = true
      defer { busy = false }
      // This run's own is told as it is now; an earlier run's, as recovery
      // tells it, with whether its VM is gone looked at again.
      let outcome =
        record.run == incarnation.id
        ? await seen(record.operation, failure: nil)
        : await recovered(record.operation, vmGone: await host.earlierGone())
      if let outcome,
        let change = record.operation.change,
        stillPending(change),
        (try? ledger.settle(record, as: outcome)) != nil
      {
        record.outcome = outcome
      }
    }
    reply.outcome = record.outcome
    reply.pending = record.outcome == nil
    return reply
  }

  /// Whether the change `change` is kept and still unsettled.
  private func stillPending(_ change: ChangeID) -> Bool {
    guard let kept = try? ledger.record(change) else { return false }
    return kept.outcome == nil
  }

  /// Do `operation` and say what it came to; none when it cannot be told,
  /// which leaves it pending.
  private func perform(_ operation: ControlOperation) async -> ChangeOutcome? {
    if case .create = operation {
      let power = await host.power()
      guard power.confirmed, power.value == .absent else {
        return .refused("The VM is made already, or whether it is cannot be told.")
      }
    }
    do {
      switch operation {
      case .hello, .state, .latest, .outcome, .void:
        return .refused("It is not a change.")
      case .create(let change):
        try await host.create(change)
      case .start:
        try await host.start()
      case .stop:
        try await host.stop()
      case .window(_, input: true, let setup):
        // Moved on before the window can take any input.
        fence.beginOpening()
        defer { fence.endOpening() }
        try await host.openWindow(setup: setup)
      case .window(_, input: false, _):
        try await host.closeWindow()
      case .install(let change, let request):
        // Told by its own phases, and closed only on the station's word.
        return await host.install(change, request)
      case .installResume(let install):
        // No secret: the install was handed it already.
        return await host.resumeInstall(install)
      case .paired(_, let install, let enrolment):
        // Only for the enrolment that install was asked for.
        guard let kept = try? ledger.record(install), case .install(_, let request) = kept.operation,
          request.enrolment == enrolment
        else {
          return .refused("That is not the enrolment of that install.")
        }
        return await host.paired(install: install)
      case .closed(_, let install):
        return await host.closed(install: install)
      case .forget:
        return await host.forget()
      }
    } catch {
      // It may have taken effect before it failed: what is there says.
      // Bounded, so its record is always read back whole.
      return await seen(
        operation,
        failure: String(decoding: Data(String(describing: error).utf8.prefix(500)), as: UTF8.self)
      )
    }
    return await seen(operation, failure: nil)
  }

  /// What `operation` came to, as what is there shows now, after it
  /// returned or, with `failure`, failed; none when that cannot be told.
  private func seen(_ operation: ControlOperation, failure: String?) async -> ChangeOutcome? {
    let refusal = ChangeOutcome.refused(failure ?? "It did not take effect.")
    switch operation {
    case .hello, .state, .latest, .outcome:
      return refusal
    case .void:
      return Self.voided
    case .create, .start, .stop:
      let power = await host.power()
      guard power.confirmed else { return nil }
      return effect(of: operation, on: power.value)
        .map { $0 ? .done : refusal }
    case .window(_, let input, _):
      // Off only once it is seen gone.
      return await host.windowOpen() == input ? .done : refusal
    case .install, .installResume, .paired, .closed, .forget:
      // Told only by doing it, or by the station's word.
      return nil
    }
  }

  /// Whether `operation` is in effect with the VM's power at `power`; none
  /// while that power cannot tell.
  private func effect(of operation: ControlOperation, on power: PowerState) -> Bool? {
    switch (operation, power) {
    case (_, .unknown), (.start, .starting), (.stop, .stopping), (.create, .starting), (.create, .stopping):
      nil
    case (.create, .absent):
      false
    case (.create, _):
      true
    case (.start, .running):
      true
    case (.start, _):
      false
    case (.stop, .stopped), (.stop, .absent):
      true
    case (.stop, _):
      false
    default:
      nil
    }
  }

  /// What `operation`, intended by a run now gone, came to, as what is
  /// there shows; none when it cannot be told.
  private func recovered(_ operation: ControlOperation, vmGone: Bool) async -> ChangeOutcome? {
    switch operation {
    case .hello, .state, .latest, .outcome:
      return nil
    case .void:
      return Self.voided
    case .create, .start, .stop:
      let power = await host.power()
      guard power.confirmed, let held = effect(of: operation, on: power.value) else { return nil }
      return held ? .done : .refused("Its runner went before it was seen done; it is not in effect.")
    case .window(_, let input, _):
      guard vmGone else { return nil }
      // No window of that run's is left: off holds, on does not.
      return input ? .refused("Its runner went; its window is closed.") : .done
    case .install, .installResume, .paired, .closed, .forget:
      // Asked again by the station, which goes on with it.
      return nil
    }
  }
}
