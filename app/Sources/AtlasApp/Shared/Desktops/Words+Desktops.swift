import AtlasService
import AtlasStatus

/// The words of the desktops, as every window names them: each desktop's title, how a VM stands in a word, and what
/// the picker's menu says of a desktop. A VM is never "the helper".
extension Words {
  /// How a desktop stands while agents can't use it yet, though it is there.
  static let needsSetup = "Needs setup"

  /// The title of `entry`: "Second Login", "VM" for `vm`, "VM 2" for `vm-2`, else its name.
  ///
  /// - Parameter entry: A desktop the service serves.
  /// - Returns: Its title.
  static func title(of entry: DesktopList.Entry) -> String {
    title(DesktopTitle.of(entry))
  }

  /// The title of the desktop a session acts on, by the same rule as a listed desktop's.
  ///
  /// - Parameter desktop: The desktop, as the session names it.
  /// - Returns: Its title.
  static func title(of desktop: HostSessionDesktop) -> String {
    title(DesktopTitle.of(name: desktop.name, kind: desktop.kind))
  }

  /// A desktop's title in words.
  ///
  /// - Parameter title: What the app calls it.
  /// - Returns: "Second Login", "VM", "VM 2", or its name.
  private static func title(_ title: DesktopTitle) -> String {
    switch title {
    case .secondLogin: "Second Login"
    case .numberedVM(1): "VM"
    case .numberedVM(let number): "VM \(number)"
    case .named(let name): name
    }
  }

  /// How a VM stands, in a word; nil while there is nothing to say.
  ///
  /// - Parameter standing: How it stands.
  /// - Returns: "Running", "Starting…", "Stopping…", "Stopped", "In maintenance", "Needs setup", or nil.
  static func standing(_ standing: DesktopStanding) -> String? {
    switch standing {
    case .running: "Running"
    case .starting: "Starting…"
    case .stopping: "Stopping…"
    case .stopped: "Stopped"
    case .maintenance: "In maintenance"
    case .needsSetup: needsSetup
    case .ready, .unknown: nil
    }
  }

  /// What the foot of the main window's sidebar says of a VM shown: "VM" and its name, then its screen's size.
  ///
  /// - Parameters:
  ///   - entry: The VM.
  ///   - display: Its screen's size, in words, such as "1920 × 1200"; nil while unknown.
  /// - Returns: "VM work · 1920 × 1200", or "VM 2" for `vm-2` with its size unknown.
  static func vmShown(_ entry: DesktopList.Entry, display: String?) -> String {
    let named =
      switch DesktopTitle.of(entry) {
      case .named(let name): "VM \(name)"
      case .secondLogin, .numberedVM: title(of: entry)
      }
    return display.map { "\(named) · \($0)" } ?? named
  }
}
