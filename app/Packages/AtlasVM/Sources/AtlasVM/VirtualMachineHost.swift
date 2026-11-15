import AtlasVMKit
import Foundation

/// The VM as the control service reaches it: each call made on the main
/// actor, where Apple's framework runs it.
final class VirtualMachineHost: VMHost {
  private let machine: Machine

  init(machine: Machine) {
    self.machine = machine
  }

  func power() async -> Observed<PowerState> { await machine.power() }

  func create() async throws { try await machine.create() }

  func start() async throws { try await machine.start() }

  func stop() async throws { try await machine.stop() }

  func openWindow() async throws { try await machine.openWindow() }

  func closeWindow() async throws { await machine.closeWindow() }

  func windowOpen() async -> Bool { await machine.windowOpen }

  func earlierGone() async -> Bool { await machine.earlierGone }

  /// A new connection to the guest's port, its descriptor this caller's to close.
  func guestConnection() async throws -> Int32 { try await machine.guestConnection() }
}
