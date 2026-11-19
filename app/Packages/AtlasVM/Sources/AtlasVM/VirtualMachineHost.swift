import AtlasVMKit
import Foundation

/// The VM as the control service reaches it: each call made on the main
/// actor, where Apple's framework runs it, and an install by its installer.
final class VirtualMachineHost: VMHost {
  private let machine: Machine
  private let installer: GuestInstaller
  private let desktop: String
  private let newest: LatestMacOS

  /// The VM `machine` is, put Atlas inside by `installer`, the newest
  /// macOS for it asked of `newest`.
  init(machine: Machine, installer: GuestInstaller, desktop: String, newest: LatestMacOS) {
    self.machine = machine
    self.installer = installer
    self.desktop = desktop
    self.newest = newest
  }

  /// Its power now, seen just now.
  func power() async -> Observed<PowerState> { await machine.power() }

  /// Make the VM for the make change `change`.
  func create(_ change: ChangeID) async throws { try await machine.create(change) }

  /// How far making it got.
  func making() async -> MakingProgress? { await machine.making }

  /// How much of its disk is used.
  func disk() async -> DiskUse { await machine.disk }

  /// The newest macOS the framework supports here.
  func latest() async -> MacOSRelease? { await newest.release() }

  /// Start it.
  func start() async throws { try await machine.start() }

  /// Stop it.
  func stop() async throws { try await machine.stop() }

  /// Open its window, its input the person's.
  func openWindow() async throws { try await machine.openWindow() }

  /// Close its window.
  func closeWindow() async throws { await machine.closeWindow() }

  /// Whether its window is open.
  func windowOpen() async -> Bool { await machine.windowOpen }

  /// Whether the VMs earlier runs ran are seen gone.
  func earlierGone() async -> Bool { await machine.earlierGone }

  /// Put Atlas inside its guest, as far as it goes.
  func install(_ change: ChangeID, _ request: InstallRequest) async -> ChangeOutcome? {
    await installer.install(change, request)
  }

  /// Go on with an install already handed its secret.
  func resumeInstall(_ change: ChangeID) async -> ChangeOutcome? {
    await installer.resume(change)
  }

  /// Take the station's word that its guest paired.
  func paired(install: ChangeID) async -> ChangeOutcome? {
    await installer.paired(install: install)
  }

  /// Take the station's word that its guest is closed.
  func closed(install: ChangeID) async -> ChangeOutcome? {
    await installer.closed(install: install)
  }

  /// Stop the VM, delete its passwords, and end this run a moment after
  /// its answer is given and kept: its lifecycle lock goes with it.
  func forget() async -> ChangeOutcome? {
    do {
      try await machine.stop()
    } catch {
      return nil
    }
    guard await machine.power().value != .running else { return nil }
    let left = GuestPassword.forget(desktop)
    DispatchQueue.main
      .asyncAfter(deadline: .now() + 1) { exit(0) }
    guard left.isEmpty else {
      return .refused("Its passwords could not be deleted without asking: \(left.joined(separator: ", ")).")
    }
    return .done
  }

  /// A new connection to the guest's port, its descriptor this caller's to close.
  func guestConnection() async throws -> Int32 { try await machine.guestConnection() }
}
