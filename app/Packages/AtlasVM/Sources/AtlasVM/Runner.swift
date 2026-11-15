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
    // Its own signing, which its peers are judged by, read before it serves
    // anyone: a run whose file went before it could is no use, and ends.
    guard CodeIdentity.remember() else { throw AtlasVMError.unsigned }
    // Before its folder is touched: the VM's one lifecycle lock, then no
    // removal under way, which a marker that cannot be read is taken for.
    try VMFolder.makePrivate(folder.parent)
    try VMFolder.makePrivate(folder.lifecycleLock.deletingLastPathComponent())
    guard let lifecycle = RunnerLock(at: folder.lifecycleLock) else { throw AtlasVMError.anotherRun }
    var marker = stat()
    guard lstat(folder.removal.path(percentEncoded: false), &marker) != 0, errno == ENOENT else {
      throw AtlasVMError.removing
    }
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
    let host = VirtualMachineHost(machine: machine, installer: installer, desktop: desktop)
    let service = ControlService(ledger: ChangeLedger(folder: folder.changes), host: host, incarnation: incarnation)
    await service.recover(holding: lock, vmGone: vmGone)
    ControlServer.start(try UnixListener(at: folder.control), service: service)
    StationJoint.start(try UnixListener(at: folder.station), host: host)
    // The locks are this run's for as long as it lives. Once an update
    // replaced its program, it gives way as soon as its VM is stopped and
    // no change is being done: launchd starts the new build in its place.
    var program = LaunchedProgram()
    while true {
      try? await Task.sleep(for: .seconds(15))
      // One not yet told is asked again, never taken for replaced.
      program = program ?? LaunchedProgram()
      if program?.replaced == true, await service.retire() {
        exit(0)
      }
      withExtendedLifetime((lifecycle, lock)) {}
    }
  }
}
