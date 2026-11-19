/// How far making a VM got, in the order it goes.
public enum MakingPhase: String, Codable, Equatable, Sendable {
  /// This Mac checked for room, and the restore image chosen.
  case checking
  /// macOS being downloaded from Apple.
  case downloading
  /// macOS being installed in the VM.
  case installing
  /// The VM made.
  case done
}
