import AtlasStatus

/// Where recovery stands as the viewer shows it: nothing needs it, its alert shows, or Not Now set the alert aside,
/// so the footer offers Recover… instead.
enum ViewerRecovery: Equatable {
  /// Nothing needs recovery.
  case none
  /// The alert shows; one running always does.
  case alert(Recovery)
  /// The alert was set aside with Not Now until its reason changes.
  case setAside
}
