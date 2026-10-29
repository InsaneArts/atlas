/// What a session is doing now, as the badge on its icon's corner shows it. A session with nothing to show, such as
/// one that is idle or has ended, has no badge: nil where one is optional.
public enum SessionBadge: CaseIterable, Sendable {
  /// It is acting on the helper now.
  case acting
  /// It is working but waiting its turn, such as while another session holds the helper.
  case waiting
  /// It needs the person before it can go on.
  case needsYou
}
