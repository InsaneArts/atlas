import AtlasService

/// How a window names the desktop it shows in its words: "the helper" for Second Login, as it always has, and a VM by
/// its title, such as "work", never "the helper".
enum DesktopNoun: Equatable {
  /// Second Login, or no desktop named yet: the helper.
  case helper
  /// A VM, by its title.
  case vm(String)

  /// The noun of the desktop the list says is shown: a VM by its title, else the helper.
  ///
  /// - Parameter list: The desktops, as last read; nil before they were read.
  /// - Returns: The noun.
  static func shown(_ list: DesktopList?) -> DesktopNoun {
    guard let list else { return .helper }
    let shown = list.desktops.first { $0.id == list.viewed } ?? list.desktops.first(where: \.isViewed)
    guard let shown, shown.kind == .vm else { return .helper }
    return .vm(Words.title(of: shown))
  }

  /// Whether it is a VM.
  var isVM: Bool { self != .helper }

  /// The desktop in a sentence: "the helper", or the VM's title.
  var name: String {
    switch self {
    case .helper: "the helper"
    case .vm(let title): title
    }
  }

  /// The desktop leading a sentence: "The helper", or the VM's title as it is.
  var leading: String {
    switch self {
    case .helper: "The helper"
    case .vm(let title): title
    }
  }
}
