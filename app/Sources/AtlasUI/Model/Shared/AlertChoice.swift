/// A button of an alert, in any window: the one that goes ahead, or the way out.
package enum AlertChoice: Hashable, Sendable {
  /// Go ahead: Recover, Switch, Log Out, Restart, Forget or Install.
  case confirm
  /// The way out: Not Now or Cancel.
  case cancel
}
