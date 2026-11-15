import AtlasVMKit
import Darwin
import Foundation

/// The station's control socket: each connection's request lines answered
/// by the control service.
enum ControlServer {
  /// Take connections from `listener` for as long as it lasts, each served
  /// by `service`.
  static func start(_ listener: UnixListener, service: ControlService) {
    Thread.detachNewThread {
      while let connection = listener.accept() {
        // Atlas's own code alone: the station, or `atlas` itself.
        guard CodeIdentity.runsAtlas(connection) else {
          close(connection)
          continue
        }
        Task {
          await LineServer.serve(connection) { line in
            await service.answer(line: line)
          }
        }
      }
    }
  }
}
