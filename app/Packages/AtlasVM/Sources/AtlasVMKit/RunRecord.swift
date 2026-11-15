import Foundation

/// The run of a VM's runner its folder was last served by, the framework's
/// process that ran its VM once there was one, and the earlier runs whose
/// VM is not yet seen gone.
public struct RunRecord: Codable, Equatable, Sendable {
  /// The run.
  public var run: Incarnation
  /// The process that ran its VM; none while it ran none, or while which
  /// one it was cannot be told.
  public var machine: MachineProcess?
  /// Earlier runs whose VM was not seen gone when this run began, each kept
  /// until it is.
  public var earlier: [RunRecord]

  /// The longest record read.
  private static let longest = 64 * 1024

  /// `run`, with the process `machine` that ran its VM, if any, and the
  /// `earlier` runs whose VM is not yet seen gone.
  public init(run: Incarnation, machine: MachineProcess? = nil, earlier: [RunRecord] = []) {
    self.run = run
    self.machine = machine
    self.earlier = earlier
  }

  private enum Key: String, CodingKey {
    case run
    case machine
    case earlier
  }

  /// Read as it is written; a record with no earlier runs may leave them out.
  public init(from decoder: any Decoder) throws {
    let fields = try decoder.container(keyedBy: Key.self)
    run = try fields.decode(Incarnation.self, forKey: .run)
    machine = try fields.decodeIfPresent(MachineProcess.self, forKey: .machine)
    earlier = try fields.decodeIfPresent([RunRecord].self, forKey: .earlier) ?? []
  }

  /// The record kept at `file`; none when there is none.
  public static func read(from file: URL) throws -> RunRecord? {
    guard let data = try PrivateFile.read(file, limit: longest) else { return nil }
    do {
      return try JSONDecoder().decode(RunRecord.self, from: data)
    } catch {
      throw PrivateFileError.unreadable(file)
    }
  }

  /// Keep it at `file`, so it outlives a crash.
  public func write(to file: URL) throws {
    guard let data = try? JSONEncoder().encode(self), data.count <= Self.longest else {
      throw PrivateFileError.unwritable(file)
    }
    try PrivateFile.write(data, to: file)
  }

  /// This run and the earlier ones it kept whose VM is not seen gone now,
  /// each without earlier runs of its own: what a run that follows keeps.
  /// None left means every VM they ran is gone. None is ever dropped: a
  /// record too long to keep stops the run that would keep it.
  public func unresolved() -> [RunRecord] {
    (earlier + [RunRecord(run: run, machine: machine)]).filter { !$0.machineGone() }
  }

  /// Whether the VM this run ran is seen gone: the process it kept seen
  /// gone; or, when it kept none, no VM process of the framework's started
  /// since this run did, so none can be one it started. Never when that
  /// cannot be told.
  public func machineGone() -> Bool {
    if let machine {
      return machine.exited() == true
    }
    guard let running = MachineProcess.running() else { return false }
    let started = (run.startSeconds, run.startMicroseconds)
    return !running.contains { ($0.startSeconds, $0.startMicroseconds) >= started }
  }
}
