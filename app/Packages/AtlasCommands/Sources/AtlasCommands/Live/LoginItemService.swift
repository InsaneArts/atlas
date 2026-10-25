import ServiceManagement

/// The system's login item registration and the operations that change it.
protocol LoginItemService: Sendable {
  /// The system's current registration status.
  var status: SMAppService.Status { get }
  /// Registers the service, or throws the system's refusal.
  func register() throws
  /// Unregisters the service, or throws the system's refusal.
  func unregister() throws
}
