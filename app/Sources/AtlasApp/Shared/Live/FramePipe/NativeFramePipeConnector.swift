import AtlasService

/// Connects to the frame service over its two Unix sockets, each checked to be served by its process and this account.
nonisolated struct NativeFramePipeConnector: FramePipeConnector {
  /// Connects off the caller's actor: a connection to a busy socket may block.
  @concurrent func connect(_ descriptor: FramePipeDescriptor) async throws -> (
    control: any FramePipeTransport, frame: any FramePipeTransport
  ) {
    let control = try NativeSocketLane.connect(to: descriptor.controlPath, servedBy: descriptor.brokerPID)
    let frame = try NativeSocketLane.connect(to: descriptor.framePath, servedBy: descriptor.brokerPID)
    return (control, frame)
  }
}
