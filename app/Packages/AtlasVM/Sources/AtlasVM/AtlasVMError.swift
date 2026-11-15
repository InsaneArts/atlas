import Foundation

/// Why Atlas VM could not do what it was asked.
enum AtlasVMError: Error, CustomStringConvertible {
  /// Another run serves this VM.
  case anotherRun
  /// The kernel did not say when this run started.
  case noIncarnation
  /// This Mac cannot run the VM, for this reason.
  case unsupported(String)
  /// The VM is not made.
  case notMade
  /// The VM is not running.
  case notRunning
  /// Its disk could not be made.
  case diskNotMade
  /// The guest's password could not be kept or read.
  case password
  /// No restore image could be had.
  case noRestoreImage

  /// What went wrong, as the person reads it.
  var description: String {
    switch self {
    case .anotherRun: "Another run of Atlas VM serves this VM."
    case .noIncarnation: "This run of Atlas VM cannot tell when it started."
    case .unsupported(let reason): reason
    case .notMade: "The VM is not made."
    case .notRunning: "The VM is not running."
    case .diskNotMade: "The VM's disk could not be made."
    case .password: "The guest's password could not be kept."
    case .noRestoreImage: "No macOS restore image could be had for this Mac."
    }
  }
}
