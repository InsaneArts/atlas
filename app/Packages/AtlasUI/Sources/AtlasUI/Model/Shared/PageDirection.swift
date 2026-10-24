/// Which way a window's pages move when one gives way to the next: on to a later page, or back to an earlier one.
public enum PageDirection: Sendable {
  /// On to a later page: the new page comes in from the right, the old one leaves to the left.
  case forward
  /// Back to an earlier page: the new page comes in from the left, the old one leaves to the right.
  case back
}
