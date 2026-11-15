/// What a runner does to its VM. The program that runs VMs gives one built
/// on Apple's framework; tests give their own.
public protocol VMHost: Sendable {
  /// The VM's power now. A VM whose making was cut short is absent.
  func power() async -> Observed<PowerState>
  /// Make the VM: its disk installed from a restore image, its identity
  /// kept in its folder.
  func create() async throws
  /// Start the VM, headless.
  func start() async throws
  /// Stop the VM.
  func stop() async throws
  /// Open the VM's window, with input.
  func openWindow() async throws
  /// Close the VM's window, returning once it is gone.
  func closeWindow() async throws
  /// Whether the VM's window is open now.
  func windowOpen() async -> Bool
  /// Whether every VM an earlier run of the runner ran is seen gone now.
  func earlierGone() async -> Bool
}
