import AtlasService

/// Whether Settings shows the row that mutes a desktop's apps: only for a desktop that has sound. Second Login's apps
/// play on this Mac's speakers; a VM has no sound, so its apps are never heard.
public enum SoundRowRule {
  /// Whether a desktop of kind `kind` has sound.
  ///
  /// - Parameter kind: The desktop's kind.
  /// - Returns: Whether its apps can be heard.
  public static func hasSound(_ kind: DesktopList.Kind) -> Bool {
    kind == .user
  }

  /// Whether the sound row shows: while some desktop listed has sound, and before the list is read, when Second
  /// Login is the one desktop there is.
  ///
  /// - Parameter list: The desktops the service serves; nil while not read.
  /// - Returns: Whether it shows.
  public static func shows(_ list: DesktopList?) -> Bool {
    guard let list, !list.desktops.isEmpty else { return true }
    return list.desktops.contains { hasSound($0.kind) }
  }
}
