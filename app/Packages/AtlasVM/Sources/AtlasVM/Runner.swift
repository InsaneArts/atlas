import AtlasVMKit
import Foundation

/// One run of Atlas VM serving one VM: the one lock taken, the earlier
/// runs' changes settled by what is there, and then its two sockets served
/// for as long as it lives.
@MainActor
enum Runner {
  /// Serve `folder`, the VM of the desktop `desktop`; it returns only when
  /// it cannot serve.
  static func serve(_ folder: VMFolder, desktop: String) async throws {
    try folder.prepare()
    guard let lock = RunnerLock(at: folder.lock) else { throw AtlasVMError.anotherRun }
    guard let incarnation = Incarnation.current() else { throw AtlasVMError.noIncarnation }
    // Read before this run's own is kept: the earlier runs whose VM is not
    // yet seen gone, kept on until each is. A record that cannot be read
    // proves nothing of its VM.
    let unresolved: [RunRecord]
    let vmGone: Bool
    switch Result(catching: { try RunRecord.read(from: folder.incarnation) }) {
    case .success(let earlier):
      unresolved = earlier?.unresolved() ?? []
      vmGone = unresolved.isEmpty
    case .failure:
      // Kept as an earlier run of its own, so its VM stays unresolved.
      unresolved = RunRecord(run: .unreadable)
        .unresolved()
      vmGone = unresolved.isEmpty
    }
    let record = RunRecord(run: incarnation, earlier: unresolved)
    try record.write(to: folder.incarnation)
    let machine = Machine(folder: folder, desktop: desktop, record: record)
    let installer = GuestInstaller(folder: folder, desktop: desktop, machine: machine)
    let host = VirtualMachineHost(machine: machine, installer: installer)
    let service = ControlService(ledger: ChangeLedger(folder: folder.changes), host: host, incarnation: incarnation)
    await service.recover(holding: lock, vmGone: vmGone)
    ControlServer.start(try UnixListener(at: folder.control), service: service)
    StationJoint.start(try UnixListener(at: folder.station), host: host)
    // The lock is this run's for as long as it lives.
    while true {
      try? await Task.sleep(for: .seconds(3600))
      withExtendedLifetime(lock) {}
    }
  }
}
