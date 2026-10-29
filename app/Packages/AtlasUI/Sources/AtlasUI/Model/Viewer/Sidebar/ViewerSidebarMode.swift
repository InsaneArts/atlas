/// How the sidebar stands at the window's leading edge: beside the card, or hidden, the card taking its room.
public enum ViewerSidebarMode: Sendable {
  /// The whole sidebar beside the card: the sessions, the Library and the helper's health.
  case full
  /// No sidebar: the card takes the window's width, its header clear of the window buttons and the sidebar toggle.
  case hidden
}
