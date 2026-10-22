/// What the panel is doing, apart from what the service reports: its picture paused, an action of its own under
/// way, what the last one said when it did not work, and its ⋯ menu open.
struct PanelSession: Equatable {
  /// Whether the person paused the panel's picture, so it reads none until resumed.
  var paused = false
  /// How many of the panel's own requests, Stops and a Resume, the service has not answered yet. Stop never waits
  /// for any; Resume waits until every one is answered, so a Stop still under way can't undo it afterwards.
  var requests = 0
  /// What the last of the panel's own actions said when it did not work; nil once another runs or the panel opens.
  var notice: String?
  /// Whether the ⋯ menu is open, so its button draws pressed.
  var moreOpen = false
}
