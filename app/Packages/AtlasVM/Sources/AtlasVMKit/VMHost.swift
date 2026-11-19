/// What a runner does to its VM. The program that runs VMs gives one built
/// on Apple's framework; tests give their own.
public protocol VMHost: Sendable {
  /// The VM's power now. A VM whose making was cut short is absent.
  func power() async -> Observed<PowerState>
  /// Make the VM for the make change `change`: its disk installed from a
  /// restore image, its identity kept in its folder, and how far it got
  /// kept as it goes.
  func create(_ change: ChangeID) async throws
  /// How far making the VM got, as last kept; none before a make was asked.
  func making() async -> MakingProgress?
  /// How much of the VM's disk is used.
  func disk() async -> DiskUse
  /// The newest macOS Apple's framework supports on this Mac; none when it
  /// cannot be told.
  func latest() async -> MacOSRelease?
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
  /// Put Atlas inside the guest for the install change `change`, as far as
  /// it goes; none while it waits on the station's word that it is closed.
  func install(_ change: ChangeID, _ request: InstallRequest) async -> ChangeOutcome?
  /// Go on with the install `change`, already handed its secret: the guest
  /// account taken out of the administrators and Remote Login turned off
  /// again, as far as its close; none while it waits on the station's word,
  /// or cannot go on yet.
  func resumeInstall(_ change: ChangeID) async -> ChangeOutcome?
  /// Take the station's word that the install `install` paired: its secret
  /// leaves the share. None while that cannot be kept.
  func paired(install: ChangeID) async -> ChangeOutcome?
  /// Take the station's word that the install `install` is closed. None
  /// while that cannot be kept.
  func closed(install: ChangeID) async -> ChangeOutcome?
  /// Stop the VM and delete its passwords, as its removal asks, then end
  /// this run once its answer is given. None while that cannot be told.
  func forget() async -> ChangeOutcome?
}
