import AtlasVMKit
import Foundation
import Virtualization

/// One VM, as Apple's framework runs it on the main thread: made, started,
/// stopped, its window opened and closed, and its guest's port reached.
@MainActor
final class Machine {
  private let folder: VMFolder
  private let desktop: String
  private let settings = VMSettings.standard
  private var record: RunRecord
  private var running: VZVirtualMachine?
  private var window: VMWindow?

  /// How long a stop asked of the guest may take before the VM is turned off.
  private static let stopping = Duration.seconds(60)
  /// The longest a made VM waits for the process that installed it to end.
  private static let installerEnding = Duration.seconds(60)

  /// The VM in `folder`, of the desktop `desktop`, served by the run
  /// `record`.
  init(folder: VMFolder, desktop: String, record: RunRecord) {
    self.folder = folder
    self.desktop = desktop
    self.record = record
  }

  /// Whether every VM an earlier run ran is seen gone now: until then, a
  /// VM this run does not run is never confirmed stopped, and none is
  /// started on its disk. Once they are, they are forgotten.
  var earlierGone: Bool {
    guard !record.earlier.isEmpty else { return true }
    guard record.earlier.allSatisfy({ $0.machineGone() }) else { return false }
    var resolved = record
    resolved.earlier = []
    if (try? resolved.write(to: folder.incarnation)) != nil {
      record = resolved
    }
    return true
  }

  /// Kept once the VM is made whole; until then it is absent.
  private var made: URL { folder.root.appending(path: "Made") }
  /// Kept once its guest's first boot was provisioned.
  private var provisioned: URL { folder.root.appending(path: "Provisioned") }
  /// Kept while the VM is quarantined: an install refused for good, so it
  /// is never started with Remote Login reachable.
  private var quarantined: URL { folder.root.appending(path: "Quarantined") }

  /// Whether `marker` is kept; none when that cannot be read, which is
  /// never taken for absent.
  private func exists(_ marker: URL) -> Bool? {
    do {
      return try PrivateFile.read(marker, limit: 0) != nil
    } catch {
      return nil
    }
  }

  /// Its power now, seen just now.
  func power() -> Observed<PowerState> {
    switch exists(made) {
    case .none:
      return Observed(.unknown, confirmed: false)
    case .some(false):
      // Absent, once no earlier run's VM can still be making it.
      return Observed(.absent, confirmed: earlierGone)
    case .some(true):
      break
    }
    // Not running here: stopped, once no earlier run's VM can still run.
    guard let running else { return Observed(.stopped, confirmed: earlierGone) }
    switch running.state {
    case .stopped, .error:
      return Observed(.stopped, confirmed: true)
    case .starting, .resuming, .restoring:
      return Observed(.starting, confirmed: true)
    case .running:
      return Observed(.running, confirmed: true)
    case .stopping, .pausing, .saving:
      return Observed(.stopping, confirmed: true)
    case .paused:
      return Observed(.unknown, confirmed: true)
    @unknown default:
      return Observed(.unknown, confirmed: false)
    }
  }

  /// Make it for the make change `change`: this Mac checked first, then
  /// its identity and an ASIF disk, then macOS installed from the newest
  /// restore image, how far it got kept as it goes. It is absent until all
  /// of that is done.
  func create(_ change: ChangeID) async throws {
    let parent = folder.root.deletingLastPathComponent()
    let log = MakingLog(file: folder.making, change: change)
    log.checking()
    // Chosen first, so the room checked is for the image that will be used.
    let choice = try await RestoreImage.choose(in: RestoreImage.folder(in: parent))
    try Preflight.check(home: folder.root, cached: choice.source == nil, among: parent)
    let image = try await RestoreImage.fetch(choice, log: log)
    let restore = try await VZMacOSRestoreImage.image(from: image)
    guard let requirements = restore.mostFeaturefulSupportedConfiguration, requirements.hardwareModel.isSupported else {
      throw AtlasVMError.unsupported("This Mac cannot run that version of macOS.")
    }
    // What a making cut short left goes first.
    for leftover in [folder.disk, folder.auxiliaryStorage, folder.hardwareModel, folder.machineIdentifier, provisioned]
    {
      try? FileManager.default
        .removeItem(at: leftover)
    }
    try PrivateFile.write(requirements.hardwareModel.dataRepresentation, to: folder.hardwareModel)
    try PrivateFile.write(VZMacMachineIdentifier().dataRepresentation, to: folder.machineIdentifier)
    try PrivateFile.write(
      Data(VZMACAddress.randomLocallyAdministered().string.utf8),
      to: MachineConfiguration.macAddress(in: folder)
    )
    _ = try VZMacAuxiliaryStorage(
      creatingStorageAt: folder.auxiliaryStorage,
      hardwareModel: requirements.hardwareModel,
      options: []
    )
    try makeDisk()
    let installing = try await RestoreImage.installing(in: RestoreImage.folder(in: parent))
    defer { withExtendedLifetime(installing) {} }
    let before = MachineProcess.running() ?? []
    try await InstallScratch.cleaning(in: InstallScratch.temporaryFolder, image: image) {
      try await install(from: image, log: log)
      // The process that installed it holds its storage until it ends, and a
      // start before then fails: it is made once that process is seen gone.
      await installerEnded(since: before)
    }
    try PrivateFile.write(Data(), to: made)
    log.done()
  }

  /// Install macOS from `image` on a VM of its own, which goes once done,
  /// how far it got told to `log`.
  private func install(from image: URL, log: MakingLog) async throws {
    let machine = VZVirtualMachine(configuration: try MachineConfiguration.make(folder, settings: settings))
    let installer = VZMacOSInstaller(virtualMachine: machine, restoringFromImageAt: image)
    let watching = installer.progress
      .observe(\.fractionCompleted, options: [.initial]) { progress, _ in
        log.installing(fraction: progress.fractionCompleted)
      }
    defer { watching.invalidate() }
    try await installer.install()
  }

  /// How far making it got, as last kept; none before a make was asked, or
  /// while that cannot be read.
  var making: MakingProgress? {
    try? MakingProgress.read(from: folder.making)
  }

  /// How much of its disk its guest filled.
  var disk: DiskUse {
    DiskUse.of(folder.disk, size: settings.diskBytes)
  }

  /// Wait, bounded, until no framework VM process started since `before` is
  /// left: past the bound a start may still fail once, and is asked again.
  private func installerEnded(since before: [MachineProcess]) async {
    let giving = ContinuousClock.now + Self.installerEnding
    while ContinuousClock.now < giving {
      if let now = MachineProcess.running(), now.allSatisfy({ before.contains($0) }) { return }
      try? await Task.sleep(for: .milliseconds(500))
    }
  }

  /// An ASIF disk of the VM's size, which takes only what its guest fills.
  private func makeDisk() throws {
    let making = Process()
    making.executableURL = URL(filePath: "/usr/sbin/diskutil")
    making.arguments = [
      "image", "create", "blank", "--fs", "none", "--format", "ASIF",
      "--size", String(settings.diskBytes), folder.disk.path(percentEncoded: false),
    ]
    making.standardOutput = FileHandle.nullDevice
    making.standardError = FileHandle.nullDevice
    try making.run()
    making.waitUntilExit()
    // Its own: diskutil makes it readable by all.
    guard making.terminationStatus == 0, chmod(folder.disk.path(percentEncoded: false), 0o600) == 0 else {
      throw AtlasVMError.diskNotMade
    }
  }

  /// Start it, headless; its guest provisioned on its first boot. The
  /// framework's process that runs it is kept with this run's record.
  func start() async throws {
    guard exists(made) == true else { throw AtlasVMError.notMade }
    guard exists(quarantined) == false else {
      throw AtlasVMError.unsupported("The VM is quarantined until Atlas is installed in it again.")
    }
    if let running, running.state == .running { return }
    guard earlierGone else {
      throw AtlasVMError.unsupported("An earlier run's VM is not yet seen gone.")
    }
    // As many as Apple's framework runs already run: said in Atlas VM's own
    // words, before the framework is asked and when it refuses for that.
    guard VMLimit.running(among: folder.parent, besides: folder.root) < VMLimit.mostRunning else {
      throw AtlasVMError.unsupported(VMLimit.reached)
    }
    // A window on the VM it replaces shows nothing of the new one.
    closeWindow()
    let machine = VZVirtualMachine(configuration: try MachineConfiguration.make(folder, settings: settings))
    let options = VZMacOSVirtualMachineStartOptions()
    guard let wasProvisioned = exists(provisioned) else { throw AtlasVMError.notMade }
    let provisioning = !wasProvisioned
    if provisioning {
      guard #available(macOS 27, *) else { throw AtlasVMError.unsupported("Provisioning needs macOS 27.") }
      let guest = VZMacGuestProvisioningOptions()
      guest.fullName = "Atlas"
      guest.username = "atlas"
      guest.password = try GuestPassword.of(desktop)
      guest.logsInAutomatically = true
      guest.enablesRemoteLogin = true
      try options.setGuestProvisioning(guest)
    }
    // The process an earlier start of this run kept is not this one's: none is
    // kept until this one is told, so recovery falls back to every process
    // started since this run did.
    record.machine = nil
    try record.write(to: folder.incarnation)
    let before = MachineProcess.running() ?? []
    running = machine
    do {
      try await machine.start(options: options)
    } catch let error as VZError where error.code == .virtualMachineLimitExceeded {
      throw AtlasVMError.unsupported(VMLimit.reached)
    }
    let started = (MachineProcess.running() ?? [])
      .filter { !before.contains($0) }
    if started.count == 1 {
      record.machine = started[0]
      try record.write(to: folder.incarnation)
    }
    if provisioning {
      try PrivateFile.write(Data(), to: provisioned)
    }
  }

  /// Stop it: its guest asked to shut down, then turned off if it has not
  /// within a minute. Its window goes with it.
  func stop() async throws {
    closeWindow()
    guard let running, running.state != .stopped, running.state != .error else { return }
    if running.canRequestStop {
      try running.requestStop()
      let giving = ContinuousClock.now + Self.stopping
      while running.state != .stopped, ContinuousClock.now < giving {
        try await Task.sleep(for: .milliseconds(500))
      }
    }
    if running.state != .stopped {
      try await running.stop()
    }
  }

  /// Open its window, with input.
  func openWindow() throws {
    guard let running, running.state == .running else { throw AtlasVMError.notRunning }
    if let window, window.isOpen, window.shows(running) { return }
    closeWindow()
    window = VMWindow(machine: running, title: "Atlas VM")
  }

  /// Close its window.
  func closeWindow() {
    window?.close()
    window = nil
  }

  /// Whether its window is open now.
  var windowOpen: Bool { window?.isOpen == true }

  /// Quarantine the VM: it starts no more until Atlas is installed again.
  func quarantine() throws {
    try PrivateFile.write(Data(), to: quarantined)
  }

  /// Lift its quarantine, as asking for an install again does. Kept when it
  /// cannot be lifted, so the VM stays unstarted.
  func release() {
    try? PrivateFile.remove(quarantined)
  }

  /// Its MAC address, as it was made with.
  var macAddress: String? {
    (try? PrivateFile.read(MachineConfiguration.macAddress(in: folder), limit: 64))
      .flatMap { $0 }
      .map { String(decoding: $0, as: UTF8.self) }
  }

  /// Whether its guest daemon answers on its port.
  func guestAnswers() async -> Bool {
    guard let connection = try? await guestConnection() else { return false }
    close(connection)
    return true
  }

  /// A new connection to its guest's port, its descriptor the caller's.
  func guestConnection() async throws -> Int32 {
    guard let running, running.state == .running,
      let device = running.socketDevices.first as? VZVirtioSocketDevice
    else {
      throw AtlasVMError.notRunning
    }
    let connection = try await device.connect(toPort: VMSettings.guestPort)
    let descriptor = dup(connection.fileDescriptor)
    connection.close()
    guard descriptor >= 0 else { throw AtlasVMError.notRunning }
    return descriptor
  }
}
