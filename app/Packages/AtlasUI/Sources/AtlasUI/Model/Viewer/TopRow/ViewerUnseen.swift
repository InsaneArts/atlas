/// What the Activity tab holds that the person has not seen (v3 V2): actions that arrived while another tab showed.
public enum ViewerUnseen: Equatable, Sendable {
  /// Nothing new, or Activity shows.
  case none
  /// New actions, every one of them done: a small blue dot after the tab's word.
  case new
  /// New actions, one of them failed: the dot turns orange.
  case failed
}
