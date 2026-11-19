import AtlasService
import AtlasStatus
import Foundation

/// The words of the desktops: Desktops in Helper's place, each desktop's title and what it is, a VM's page, its Stop
/// alert and what its commands say. A VM is never "the helper".
extension SettingsWords {
  /// The page in Helper's place once more than one desktop is set up.
  static let desktops = "Desktops"
  /// What Second Login is, under its title.
  static let secondLoginDetail = "A hidden second login on this Mac"
  /// What a VM is, under its title and on its card.
  static let vmDetail = "A virtual Mac on this Mac"
  /// The VM page's first section: what the person does with it.
  static let useSection = "Power and Maintenance"
  /// The VM page's second section: what it holds.
  static let aboutSection = "Disk and Software"
  /// What the power row says a VM does.
  static let startsByItself = "Starts by itself when an agent needs it"
  /// The power row's button while the VM runs.
  static let stop = "Stop…"
  /// The power row's button while the VM is stopped.
  static let start = "Start"
  /// The maintenance row's title.
  static let maintenanceTitle = "Open for maintenance"
  /// What maintenance is, while it is not on.
  static let maintenanceDetail = "Use the VM yourself, such as to update macOS. Agents wait."
  /// What maintenance means while it is on.
  static let maintenanceOn = "Agents wait until you end it"
  /// The maintenance row's button while it is not on.
  static let openMaintenance = "Open…"
  /// The maintenance row's button while it is on.
  static let endMaintenance = "End…"
  /// The disk row's title.
  static let disk = "Disk"
  /// The Atlas row's title.
  static let atlasInVM = "Atlas in the VM"
  /// The remove row's title.
  static let removeVMTitle = "Remove this VM"
  /// What removing a VM does.
  static let removeVMDetail = "Deletes the VM and everything in it"
  /// The remove row's button.
  static let removeVM = "Remove…"
  /// The Stop alert's message.
  static let stopVMMessage = "Anything agents are doing there stops. It starts again when an agent needs it."
  /// The Stop alert's button.
  static let stopVM = "Stop"
  /// The Stop alert's button while the VM stops.
  static let stoppingVM = "Stopping…"
  /// What a maintenance or a removal waits on while it runs.
  static let approving = "Allow it in the approval window to go on."

  /// The title of `entry`: "Second Login", "VM" for `vm`, "VM 2" for `vm-2`, else its name.
  ///
  /// - Parameter entry: A desktop the service serves.
  /// - Returns: Its title.
  static func title(of entry: DesktopList.Entry) -> String {
    switch DesktopTitle.of(entry) {
    case .secondLogin: "Second Login"
    case .numberedVM(1): "VM"
    case .numberedVM(let number): "VM \(number)"
    case .named(let name): name
    }
  }

  /// What a desktop is, with how it stands after it when there is a word for it.
  ///
  /// - Parameters:
  ///   - entry: A desktop the service serves.
  ///   - standing: How it stands.
  /// - Returns: "A virtual Mac on this Mac · Running", or "A hidden second login on this Mac".
  static func detail(of entry: DesktopList.Entry, standing: DesktopStanding) -> String {
    guard entry.kind == .vm else { return secondLoginDetail }
    return [vmDetail, word(standing)]
      .compactMap { $0 }
      .joined(separator: " · ")
  }

  /// How a VM stands, in a word, after what it is; nil while there is nothing to say.
  ///
  /// - Parameter standing: How it stands.
  /// - Returns: "Running", "Starting…", "Stopping…", "Stopped", "In maintenance", "Needs setup", or nil.
  static func word(_ standing: DesktopStanding) -> String? {
    switch standing {
    case .running: "Running"
    case .starting: "Starting…"
    case .stopping: "Stopping…"
    case .stopped: "Stopped"
    case .maintenance: "In maintenance"
    case .needsSetup: helperNeedsSetup
    case .ready, .unknown: nil
    }
  }

  /// A VM's power, as its power row's title.
  ///
  /// - Parameter power: Its power.
  /// - Returns: "Running", "Starting…", "Stopping…" or "Stopped".
  static func power(_ power: VMPower) -> String {
    switch power {
    case .running: "Running"
    case .starting: "Starting…"
    case .stopping: "Stopping…"
    case .stopped: "Stopped"
    }
  }

  /// The Desktops row's line while a desktop needs setup.
  ///
  /// - Parameter title: The desktop's title.
  /// - Returns: "work needs setup"
  static func needsSetup(_ title: String) -> String {
    "\(title) needs setup"
  }

  /// A VM's card line: what it is, with its screen's size when it is known.
  ///
  /// - Parameter display: Its screen's size, in words, such as "1920 × 1200".
  /// - Returns: "A virtual Mac on this Mac · 1920 × 1200"
  static func vmLine(display: String?) -> String {
    [vmDetail, display]
      .compactMap { $0 }
      .joined(separator: " · ")
  }

  /// How much of a VM's disk is used, of the most it may grow to.
  ///
  /// - Parameters:
  ///   - used: The bytes used; nil while not said.
  ///   - size: The most it may grow to, in bytes.
  /// - Returns: "28 GB used of 128 GB", or "Up to 128 GB" while its use is not said.
  static func diskUse(used: Int64?, size: Int64) -> String {
    guard let used else { return "Up to \(gigabytes(size))" }
    return "\(gigabytes(used)) used of \(gigabytes(size))"
  }

  /// A VM's macOS row's title.
  ///
  /// - Parameter version: Its version.
  /// - Returns: "macOS 27.2.1"
  static func macOS(_ version: String) -> String {
    "macOS \(version)"
  }

  /// What the macOS row says while a newer macOS is out.
  ///
  /// - Parameter version: The newer version.
  /// - Returns: "macOS 27.3 is available. Open the VM for maintenance and update it in Software Update."
  static func newerMacOS(_ version: String) -> String {
    "macOS \(version) is available. Open the VM for maintenance and update it in Software Update."
  }

  /// How a VM's Atlas stands, at its row's end.
  ///
  /// - Parameter state: How it stands.
  /// - Returns: "Up to date", "Update available", "Newer than this Mac’s", "Updating…" or "Not known".
  static func atlas(_ state: VMPageRow.AtlasState) -> String {
    switch state {
    case .current: "Up to date"
    case .available: "Update available"
    case .newer: "Newer than this Mac’s"
    case .updating: "Updating…"
    case .unknown: "Not known"
    }
  }

  /// The Stop alert's title.
  ///
  /// - Parameter title: The VM's title.
  /// - Returns: "Stop work?"
  static func stopVMTitle(_ title: String) -> String {
    "Stop \(title)?"
  }

  /// What a VM's page says while its Start runs.
  ///
  /// - Parameter title: The VM's title.
  /// - Returns: "Starting work…"
  static func starting(_ title: String) -> String {
    "Starting \(title)…"
  }

  /// What a VM's page says when its Start or Stop was refused.
  ///
  /// - Parameters:
  ///   - title: The VM's title.
  ///   - start: Whether it was the Start.
  ///   - message: What the service said.
  /// - Returns: "Atlas couldn't stop work. work is open for maintenance; end its maintenance before stopping it."
  static func powerFailed(_ title: String, start: Bool, _ message: String) -> String {
    "Atlas couldn't \(start ? "start" : "stop") \(title). \(Words.sentence(message))"
  }

  /// What Desktops says once a VM was removed.
  ///
  /// - Parameter title: The VM's title.
  /// - Returns: "work was removed."
  static func removed(_ title: String) -> String {
    "\(title) was removed."
  }

  /// A count of bytes as whole gigabytes, as Atlas VM counts them.
  ///
  /// - Parameter bytes: The bytes.
  /// - Returns: "28 GB"
  private static func gigabytes(_ bytes: Int64) -> String {
    let gigabytes = Double(bytes) / 1_000_000_000
    let number = gigabytes.formatted(
      .number
        .precision(.fractionLength(0))
        .locale(Locale(identifier: "en_US_POSIX"))
    )
    return "\(number) GB"
  }
}
