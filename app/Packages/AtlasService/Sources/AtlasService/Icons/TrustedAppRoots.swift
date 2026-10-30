/// Where apps are installed, the only places an app's icon is read from: the system's and the person's Applications
/// folders and the system's own apps, nowhere another app could have put a bundle. The rule the service keeps for the
/// icons it sends, and one root more: the system's own apps that macOS updates apart from the system, such as Safari,
/// which `/Applications/Safari.app` links to.
public enum TrustedAppRoots {
  /// The folders under the system's root that hold apps: the Applications folder, the system's apps, Finder's and the
  /// system's other own apps, and the apps macOS updates apart from the system, all but the first sealed by macOS.
  static let systemRoots = [
    "/Applications", "/System/Applications", "/System/Library/CoreServices",
    "/System/Cryptexes/App/System/Applications",
  ]
  /// The folders an app is looked for by its name, in order: the roots and the system's Utilities.
  static let systemFolders = [
    "/Applications", "/System/Applications", "/System/Applications/Utilities", "/System/Library/CoreServices",
  ]
  /// The person's own Applications folder, inside their home.
  static let homeApplications = "Applications"
  /// What ends an app bundle's name.
  public static let bundleSuffix = ".app"

  /// The roots apps are installed under.
  ///
  /// - Parameter home: The person's home folder.
  /// - Returns: The system's roots, then the person's Applications folder.
  public static func roots(home: String) -> [String] {
    systemRoots + [joined(home, homeApplications)]
  }

  /// The folders an app is looked for by its name, in order.
  ///
  /// - Parameter home: The person's home folder.
  /// - Returns: The system's folders, then the person's Applications folder.
  public static func folders(home: String) -> [String] {
    systemFolders + [joined(home, homeApplications)]
  }

  /// Whether a path lies under a root apps are installed under: absolute, with no `..`, and inside a root by whole
  /// folder names, so `/ApplicationsEvil` is not inside `/Applications`.
  ///
  /// - Parameters:
  ///   - path: The path, such as an app's bundle.
  ///   - home: The person's home folder.
  /// - Returns: Whether an icon may be read from it.
  public static func contains(_ path: String, home: String) -> Bool {
    guard path.hasPrefix("/") else { return false }
    let parts = components(path)
    guard !parts.contains("..") else { return false }
    return roots(home: home)
      .map(components)
      .contains { root in !root.isEmpty && parts.count >= root.count && Array(parts.prefix(root.count)) == root }
  }

  /// The bundle an app of a name would be in, in a folder: `<Name>.app`.
  ///
  /// - Parameters:
  ///   - name: The app's name, which must hold no slash.
  ///   - folder: The folder.
  /// - Returns: The bundle's path, or nil for a name that is not a bare file name.
  public static func bundle(named name: String, in folder: String) -> String? {
    guard !name.isEmpty, !name.contains("/"), !name.contains(".."), name != "." else { return nil }
    return joined(folder, name + bundleSuffix)
  }

  /// A path's folder names, without the empty and `.` parts a path may hold.
  ///
  /// - Parameter path: The path.
  /// - Returns: Its folder names, in order.
  static func components(_ path: String) -> [String] {
    path.split(separator: "/")
      .map(String.init)
      .filter { $0 != "." }
  }

  /// A folder and a name inside it as one path.
  ///
  /// - Parameters:
  ///   - folder: The folder.
  ///   - name: The name.
  /// - Returns: The path.
  static func joined(_ folder: String, _ name: String) -> String {
    folder.hasSuffix("/") ? folder + name : folder + "/" + name
  }
}
