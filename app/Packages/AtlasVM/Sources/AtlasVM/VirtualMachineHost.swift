import AtlasVMKit
import Foundation

/// The VM as the control service reaches it: each call made on the main
/// actor, where Apple's framework runs it, and an install by its installer.
final class VirtualMachineHost: VMHost {
  private let machine: Machine
  private let installer: GuestInstaller

  /// The VM `machine` is, put Atlas inside by `installer`.
  init(machine: Machine, installer: GuestInstaller) {
    self.machine = machine
    self.installer = installer
  }

  /// Its power now, seen just now.
  func power() async -> Observed<PowerState> { await machine.power() }

  /// Make the VM.
  func create() async throws { try await machine.create() }

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

  /// Take the station's word that its guest paired.
  func paired(install: ChangeID) async -> ChangeOutcome? {
    await installer.paired(install: install)
  }

  /// Take the station's word that its guest is closed.
  func closed(install: ChangeID) async -> ChangeOutcome? {
    await installer.closed(install: install)
  }

  /// A new connection to the guest's port, its descriptor this caller's to close.
  func guestConnection() async throws -> Int32 { try await machine.guestConnection() }
}
