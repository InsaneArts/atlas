import AtlasVMKit
import Foundation

/// What this Mac must have before a VM is made: checked before any download
/// or disk change, each failure saying why and changing nothing.
enum Preflight {
  /// The most macOS VMs Apple's framework runs at once.
  private static let mostRunning = 2

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
    // The framework's processes do not say which guests are macOS, so the
    // count is of Atlas's own; another app's are the framework's to refuse.
    guard running(among: parent) < mostRunning else {
      throw AtlasVMError.unsupported("This Mac already runs \(mostRunning) macOS VMs, as many as it may.")
    }
  }

  /// How many of Atlas's VMs in `parent` may be running: each whose run
  /// record names a VM process still running, or cannot be read.
  private static func running(among parent: URL) -> Int {
    let names = (try? FileManager.default.contentsOfDirectory(atPath: parent.path(percentEncoded: false))) ?? []
    return names.compactMap { VMFolder(in: parent, desktop: $0) }
      .filter { folder in
        guard let record = try? RunRecord.read(from: folder.incarnation) else {
          return FileManager.default
            .fileExists(atPath: folder.incarnation.path(percentEncoded: false))
        }
        return record.machine?.exited() == false
      }.count
  }
}
