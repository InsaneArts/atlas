/// The viewer's footer: where things stand on the left, the next step on the right.
package struct ViewerFooterState: Equatable, Sendable {
  /// What the person can do now, in one line.
  package var hint: String
  /// Whether Recover… shows: a recovery alert was set aside with Not Now.
  package var offersRecovery: Bool
  /// The pause button: on while the picture is paused.
  package var pause: ViewerToggle
  /// Whether Keys… can be pressed: only while this window holds control.
  package var keysEnabled: Bool
  /// Take Control, or Give Back while in control.
  package var control: ViewerControlButton
  /// Stop Agent or Resume Agent, when either applies.
  package var agent: ViewerAgentButton?

  /// Creates the footer's state.
  ///
  /// - Parameters:
  ///   - hint: What the person can do now, in one line.
  ///   - offersRecovery: Whether Recover… shows: a recovery alert was set aside with Not Now.
  ///   - pause: The pause button: on while the picture is paused.
  ///   - keysEnabled: Whether Keys… can be pressed: only while this window holds control.
  ///   - control: Take Control, or Give Back while in control.
  ///   - agent: Stop Agent or Resume Agent, when either applies.
  package init(
    hint: String,
    offersRecovery: Bool,
    pause: ViewerToggle,
    keysEnabled: Bool,
    control: ViewerControlButton,
    agent: ViewerAgentButton? = nil
  ) {
    self.hint = hint
    self.offersRecovery = offersRecovery
    self.pause = pause
    self.keysEnabled = keysEnabled
    self.control = control
    self.agent = agent
  }
}
