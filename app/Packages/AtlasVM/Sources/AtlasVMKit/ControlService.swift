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
  /// The lock this run serves under, once it recovered: its proof that
  /// every earlier run has ended.
  private var lock: RunnerLock?

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
      let power = await host.power()
      var reply = ControlReply.served("state", by: incarnation)
      reply.power = power.value
      reply.confirmed = power.confirmed
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
    case .create, .start, .stop, .window:
      return await change(operation)
    }
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
  }

  private func change(_ operation: ControlOperation) async -> ControlReply {
    var reply = ControlReply.served(operation.name, by: incarnation)
    reply.change = operation.change
    let beginning: ChangeBeginning
    do {
      // Asked again, it is answered from its record even while another runs.
      if let change = operation.change, let kept = try ledger.record(change) {
        guard kept.operation == operation else {
          return .refused("Another change was asked with this ID.", by: incarnation)
        }
        return await answered(kept, in: reply)
      }
      guard !busy else {
        return .refused("Another change is being done; ask again once it is.", by: incarnation)
      }
      beginning = try ledger.begin(operation, by: incarnation.id)
    } catch {
      return .refused("The change could not be kept, so nothing was done.", by: incarnation)
    }
    switch beginning {
    case .conflicting:
      return .refused("Another change was asked with this ID.", by: incarnation)
    case .asked(let record):
      return await answered(record, in: reply)
    case .new:
      busy = true
      defer { busy = false }
      let record = ChangeRecord(operation: operation, run: incarnation.id)
      guard let outcome = await perform(operation) else {
        reply.pending = true
        return reply
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
  }

  /// `reply`, answering for `record`: one this run left pending, its
  /// effect not seen or not kept then, is looked at again first and settled
  /// if what is there now tells.
  private func answered(_ record: ChangeRecord, in reply: ControlReply) async -> ControlReply {
    var reply = reply
    var record = record
    if record.outcome == nil, record.run == incarnation.id || lock != nil, !busy {
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
      case .hello, .state, .outcome:
        return .refused("It is not a change.")
      case .create:
        try await host.create()
      case .start:
        try await host.start()
      case .stop:
        try await host.stop()
      case .window(_, input: true):
        try await host.openWindow()
      case .window(_, input: false):
        try await host.closeWindow()
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
    case .hello, .state, .outcome:
      return refusal
    case .create, .start, .stop:
      let power = await host.power()
      guard power.confirmed else { return nil }
      return effect(of: operation, on: power.value).map { $0 ? .done : refusal }
    case .window(_, let input):
      // Off only once it is seen gone.
      return await host.windowOpen() == input ? .done : refusal
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
    case .hello, .state, .outcome:
      return nil
    case .create, .start, .stop:
      let power = await host.power()
      guard power.confirmed, let held = effect(of: operation, on: power.value) else { return nil }
      return held ? .done : .refused("Its runner went before it was seen done; it is not in effect.")
    case .window(_, let input):
      guard vmGone else { return nil }
      // No window of that run's is left: off holds, on does not.
      return input ? .refused("Its runner went; its window is closed.") : .done
    }
  }
}
