/// How to pair with the frame service: its sockets, its instance and the observer capability. The capability is a
/// bearer secret, like the viewer's address: never logged or shown.
public struct FramePipeDescriptor: Sendable, Equatable, Decodable {
  /// The control lane's socket path.
  public let controlPath: String
  /// The frame lane's socket path.
  public let framePath: String
  /// The service instance, 32 lowercase hex digits.
  public let serviceInstance: String
  /// The observer capability, 64 lowercase hex digits.
  public let observerBootstrap: String
  /// The process that serves both lanes, which the kernel must name as each lane's peer.
  public let brokerPID: Int32
  /// The account that serves them: this app's own.
  public let controllerUID: UInt32

  /// Creates a descriptor.
  ///
  /// - Parameters:
  ///   - controlPath: The control lane's socket path.
  ///   - framePath: The frame lane's socket path.
  ///   - serviceInstance: The service instance.
  ///   - observerBootstrap: The observer capability.
  ///   - brokerPID: The process that serves both lanes.
  ///   - controllerUID: The account that serves them.
  public init(
    controlPath: String,
    framePath: String,
    serviceInstance: String,
    observerBootstrap: String,
    brokerPID: Int32,
    controllerUID: UInt32
  ) {
    self.controlPath = controlPath
    self.framePath = framePath
    self.serviceInstance = serviceInstance
    self.observerBootstrap = observerBootstrap
    self.brokerPID = brokerPID
    self.controllerUID = controllerUID
  }
}
