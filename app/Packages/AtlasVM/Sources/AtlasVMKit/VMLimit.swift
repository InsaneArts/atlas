import Foundation

/// How many macOS VMs Apple's framework runs on one Mac at once, and how
/// Atlas VM says it when one more is asked of it.
public enum VMLimit {
  /// The most macOS VMs Apple's framework runs at once.
  public static let mostRunning = 2

  /// Why a VM is not made or started while as many already run.
  public static let reached = "This Mac already runs \(mostRunning) macOS VMs, as many as it may."

  /// How many of Atlas's VMs in `parent` may be running, the one in
  /// `besides` not counted: each whose run record names a VM process still
  /// running, or cannot be read. The framework's processes do not say which
  /// guests are macOS, so the count is of Atlas's own; another app's are the
  /// framework's to refuse.
  public static func running(among parent: URL, besides: URL? = nil) -> Int {
    let names = (try? FileManager.default.contentsOfDirectory(atPath: parent.path(percentEncoded: false))) ?? []
    return names.compactMap { VMFolder(in: parent, desktop: $0) }
      .filter { $0.root.standardizedFileURL != besides?.standardizedFileURL }
      .filter { folder in
        guard let record = try? RunRecord.read(from: folder.incarnation) else {
          return FileManager.default
            .fileExists(atPath: folder.incarnation.path(percentEncoded: false))
        }
        return record.machine?.exited() == false
      }
      .count
  }
}
