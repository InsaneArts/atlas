/// What ends a permission's row (v3 D4, Q-D3): its Allow… while it is the one to allow next, a check and Allowed once
/// on, a spinner and Checking… while it cannot be read, else a quiet Off.
public enum OnboardingPermissionStanding: Hashable, Sendable {
  /// Allow…, which opens its list in System Settings.
  case allow(String)
  /// On, in words.
  case allowed(String)
  /// Not read yet, in words: never shown as off.
  case checking(String)
  /// Off, and not the one to allow next.
  case off(String)

  /// Where `permission` stands on its row.
  ///
  /// - Parameters:
  ///   - permission: The permission.
  ///   - offersAllow: Whether the row takes a press: the screen's rows do, the panel's only show.
  /// - Returns: Its standing.
  public static func of(_ permission: OnboardingPermissionItem, offersAllow: Bool) -> OnboardingPermissionStanding {
    if permission.isOn { return .allowed(permission.status) }
    if !permission.isKnown { return .checking(permission.status) }
    if offersAllow && permission.canAllow { return .allow(permission.allowTitle) }
    return .off(permission.status)
  }
}
