import AtlasOnboarding
import Foundation

/// The words of the choice of where agents work and of the VM's path that depend on what Atlas reports: why a kind
/// cannot be chosen, how much of macOS is down, why the add is held, and Done's paragraph with a VM.
extension OnboardingWords {
  /// The add's process ended before the VM was set up.
  static let vmStopped = "Setting up the VM stopped."
  /// Done's paragraph with a VM: no login to come back to after a restart.
  static let vmDoneLede =
    "Atlas lives in your menu bar. Ask your agent to check something on screen and watch it work there."

  /// Why a kind of desktop cannot be chosen now, at the end of its row.
  ///
  /// - Parameter lack: What it lacks.
  /// - Returns: "Not available yet", or what a VM needs.
  static func lack(_ lack: DesktopChoice.Lack) -> String {
    switch lack {
    case .notYet: "Not available yet"
    case .macOS: "Needs macOS 27"
    case .chip: "Needs Apple silicon"
    case .disk: "Needs 70 GB free"
    case .running: "Two VMs already run"
    }
  }

  /// How much of macOS is down, of the whole, in gigabytes to one place, as Atlas VM counts them.
  ///
  /// - Parameter progress: Atlas VM's word on making the VM.
  /// - Returns: "12.4 GB of 25.6 GB", or nil while either is not known.
  static func downloaded(_ progress: VMFacts.Progress?) -> String? {
    guard let received = progress?.received, let total = progress?.total, total > 0 else { return nil }
    return "\(gigabytes(received)) of \(gigabytes(total))"
  }

  /// Why the VM's add failed, in its own words: as a clause after the window's, without the terminal's way to go on,
  /// which Try Again is.
  ///
  /// - Parameter reason: The add's reason.
  /// - Returns: "Atlas couldn’t make the VM: the VM’s disk could not be made."
  static func vmFailed(_ reason: String) -> String {
    let kept =
      reason.components(separatedBy: " Run atlas desktop add vm again")
      .first ?? reason
    return "Atlas couldn’t make the VM: \(Words.sentence(clause(kept)))"
  }

  /// `text` set to follow a colon: its first word in lower case when it is an ordinary capitalised word, never a name
  /// such as Atlas or a word in capitals such as VM.
  ///
  /// - Parameter text: A sentence.
  /// - Returns: The clause.
  static func clause(_ text: String) -> String {
    let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
    guard let first = trimmed.split(separator: " ").first, first.count > 1, first != "Atlas",
      first.dropFirst().allSatisfy({ $0.isLowercase || !$0.isLetter })
    else { return trimmed }
    let initial = trimmed.prefix(1)
      .lowercased()
    return initial + trimmed.dropFirst()
  }

  /// A count of bytes as gigabytes to one place.
  ///
  /// - Parameter bytes: The bytes.
  /// - Returns: "12.4 GB"
  private static func gigabytes(_ bytes: Int64) -> String {
    let gigabytes = Double(bytes) / 1_000_000_000
    let number = gigabytes.formatted(
      .number
        .precision(.fractionLength(1))
        .locale(Locale(identifier: "en_US_POSIX"))
    )
    return "\(number) GB"
  }
}
