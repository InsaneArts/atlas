/// What the Library's Activity row holds that the person has not seen: actions that arrived while another place showed.
public enum ViewerUnseen: Equatable, Sendable {
  /// Nothing new, or Activity shows.
  case none
  /// New actions, every one of them done: a small blue dot after the row's word.
  case new
  /// New actions, one of them failed: the dot turns orange.
  case failed
}
