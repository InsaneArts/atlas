/// The phases of putting Atlas inside a guest, in the order they are done.
public enum InstallPhase: Int, Codable, Sendable {
  /// The bundle archived once, frozen.
  case archived
  /// The archive sent to the guest and unpacked by its account.
  case uploaded
  /// Atlas published as `/Applications/Atlas.app` by root.
  case published
  /// The machine job and the root installer written and started.
  case jobs
  /// The second administrator made.
  case admin
  /// The guest account taken out of the administrators.
  case demoted
  /// Remote Login being turned off.
  case closing
  /// The station said the guest reports Remote Login off.
  case closed
}
