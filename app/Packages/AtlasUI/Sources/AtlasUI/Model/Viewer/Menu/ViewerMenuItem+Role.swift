extension ViewerMenuItem {
  /// What choosing a menu item does.
  public enum Role: Equatable, Sendable {
    /// An ordinary command.
    case normal
    /// A command that destroys something, such as Clean Up This Session's Work…, which the menu draws as such.
    case destructive
  }
}
