import AtlasVMKit
import Foundation

/// What this Mac must have before a VM is made: checked before any download
/// or disk change, each failure saying why and changing nothing.
enum Preflight {
  /// Check this Mac can make a VM in `home`'s volume, the restore image
  /// `cached` or yet to be downloaded, beside the VMs in `parent`.
  static func check(home: URL, cached: Bool, among parent: URL) throws {
    guard ProcessInfo.processInfo.operatingSystemVersion.majorVersion >= 27 else {
      throw AtlasVMError.unsupported("A VM needs macOS 27 or later on this Mac.")
    }
    #if !arch(arm64)
      throw AtlasVMError.unsupported("A VM needs a Mac with Apple silicon.")
    #endif
    // About 28 GB once installed, and the restore image's 26 GB to download.
    let needed: Int64 = cached ? 40_000_000_000 : 70_000_000_000
    let free = try home.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey])
      .volumeAvailableCapacityForImportantUsage
    guard let free, free >= needed else {
      throw AtlasVMError.unsupported("This Mac needs \(needed / 1_000_000_000) GB free to make a VM.")
    }
    guard VMLimit.running(among: parent) < VMLimit.mostRunning else {
      throw AtlasVMError.unsupported(VMLimit.reached)
    }
  }
}
