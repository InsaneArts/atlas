/// The newest action the person has seen (v3 V2): whatever is newer in the helper's activity is unseen, and Activity's
/// tab carries a dot for it until it is chosen.
struct ViewerSeenActivity: Equatable {
  /// The newest action's id when it was seen; nil when there was none, so every action since is unseen.
  var newest: String?
}
