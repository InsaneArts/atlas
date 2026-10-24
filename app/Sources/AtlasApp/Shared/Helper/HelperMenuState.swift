/// Which of the helper's commands its menu items offer now, as a window's presenter decides, so every menu that
/// lists them, and every button that runs one, agree.
struct HelperMenuState: Equatable {
  /// Whether Recover… would help: recovery is offered and none runs.
  var canRecover: Bool
  /// Whether Restart the Helper… can run: the helper answers and no restart runs.
  var canRestart: Bool
  /// Whether Switch to the Helper… can run: no switch runs.
  var canSwitch: Bool
  /// Whether Log Out the Helper… can run: the helper's mode allows it and no log-out runs.
  var canLogOut: Bool
}
