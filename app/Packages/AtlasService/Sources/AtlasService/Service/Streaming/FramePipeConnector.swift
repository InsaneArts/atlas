/// Opens both lanes to a frame service, each checked to be served by the descriptor's process and account.
public protocol FramePipeConnector: Sendable {
  /// The control and frame lanes to the service `descriptor` names.
  ///
  /// - Parameter descriptor: The service.
  /// - Returns: Both lanes, connected and checked.
  /// - Throws: When either cannot be connected or another process serves it.
  func connect(_ descriptor: FramePipeDescriptor) async throws -> (
    control: any FramePipeTransport, frame: any FramePipeTransport
  )
}
