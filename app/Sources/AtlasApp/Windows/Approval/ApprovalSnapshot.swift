import AtlasCommands
import AtlasUI

/// What the approval window's controller holds, as one plain value for ``ApprovalPresenter``.
struct ApprovalSnapshot {
  /// The request as the command line read it; nil until it is read, or when it could not be.
  var request: LifecycleRequest?
  /// Where the answer stands.
  var phase: ApprovalPhase = .loading
}
