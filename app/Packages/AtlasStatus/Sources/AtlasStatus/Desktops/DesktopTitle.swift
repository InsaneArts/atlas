import AtlasService

/// What the app calls a desktop: Second Login by its kind, a VM by its name in the settings, except the names Atlas
/// gives a VM itself, `vm` and `vm-2` on, which stand for the first VM, the second and on. Never "the helper" for a VM.
public enum DesktopTitle: Equatable, Sendable {
  /// The hidden second login on this Mac.
  case secondLogin
  /// The VM Atlas named for its place: 1 for `vm`, 2 for `vm-2`.
  case numberedVM(Int)
  /// A desktop by the name the person gave it.
  case named(String)

  /// The title of `entry`.
  ///
  /// - Parameter entry: A desktop the service serves.
  /// - Returns: Its title.
  public static func of(_ entry: DesktopList.Entry) -> DesktopTitle {
    of(name: entry.name, kind: entry.kind)
  }

  /// The title of a desktop named `name` in the settings, of kind `kind`.
  ///
  /// - Parameters:
  ///   - name: Its name in the settings.
  ///   - kind: Its kind.
  /// - Returns: Its title.
  public static func of(name: String, kind: DesktopList.Kind) -> DesktopTitle {
    switch kind {
    case .user: return .secondLogin
    case .vm:
      if name == "vm" { return .numberedVM(1) }
      let number = name.hasPrefix("vm-") ? Int(name.dropFirst("vm-".count)) : nil
      guard let number, number >= 2, "vm-\(number)" == name else { return .named(name) }
      return .numberedVM(number)
    case .unknown: return .named(name)
    }
  }
}
