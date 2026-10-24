/// Which way setup moves when its screen changes: on to a later screen, or back to an earlier one.
public enum StepDirection: Sendable {
  /// On to a later screen, or to one that has no place in the order, such as a blocked screen.
  case forward
  /// Back to an earlier screen: Back, or a helper let go to choose another.
  case back
}
