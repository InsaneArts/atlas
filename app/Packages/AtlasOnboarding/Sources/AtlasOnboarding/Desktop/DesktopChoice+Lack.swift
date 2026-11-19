extension DesktopChoice {
  /// Why a kind cannot be chosen now.
  public enum Lack: Equatable, Sendable {
    /// Atlas does not offer it yet.
    case notYet
    /// A VM needs macOS 27 or later on this Mac.
    case macOS
    /// A VM needs Apple silicon that runs virtual machines.
    case chip
    /// A VM needs 70 GB free.
    case disk
    /// Two VMs already run on this Mac, as many as it may.
    case running

    /// What `atlas onboarding status` says a VM lacks, as its `why`: an unknown word reads as not offered yet.
    ///
    /// - Parameter why: The word.
    init(why: String?) {
      switch why {
      case "macos": self = .macOS
      case "chip": self = .chip
      case "disk": self = .disk
      case "running": self = .running
      default: self = .notYet
      }
    }
  }
}
