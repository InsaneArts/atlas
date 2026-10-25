/// What the panel is doing, apart from what the service reports: its picture paused, what the last action said
/// when it did not work, its ⋯ menu open, and what VoiceOver should say.
struct PanelSession: Equatable {
  /// Whether the person paused the panel's picture, so it reads none until resumed.
  var paused = false
  /// What the last of the panel's own actions said when it did not work; nil once another runs or the panel opens.
  var notice: String?
  /// Whether the ⋯ menu is open, so its button draws pressed.
  var moreOpen = false
  /// What VoiceOver should say: a state that asks something of the person, once it has held for
  /// ``PanelController/announceAfter``; nil for nothing.
  var announcement: String?
}
