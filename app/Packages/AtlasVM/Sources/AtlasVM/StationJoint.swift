import AtlasVMKit
import Darwin
import Foundation

/// The station's session socket: each connection joined to a new vsock
/// connection to the guest's port, bytes only.
enum StationJoint {
  /// Take connections from `listener` for as long as it lasts, each joined
  /// to the guest that `host` runs.
  static func start(_ listener: UnixListener, host: VirtualMachineHost) {
    Thread.detachNewThread {
      while let connection = listener.accept() {
        Task {
          do {
            let guest = try await host.guestConnection()
            Splice.join(connection, guest)
          } catch {
            close(connection)
          }
        }
      }
    }
  }
}
