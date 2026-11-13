import Darwin
import Foundation

/// The one running Atlas app of this account: a lock held for as long as the app runs, and a record beside it naming
/// the app's process, so updates and removal find it with no service running.
nonisolated enum AppLock {
  /// The person's Atlas folder.
  static var folder: URL {
    FileManager.default.homeDirectoryForCurrentUser
      .appending(path: "Library/Application Support/Atlas")
  }

  /// How long a new app waits for one that is leaving, in seconds: one that found itself started for another service
  /// leaves the lock to the app started for it.
  static let handover: TimeInterval = 10

  /// Takes the lock for this app for as long as it runs, and records its process; false while another app holds it
  /// past `waiting` seconds, or when it can't be recorded.
  ///
  /// - Parameter waiting: How long to wait for another app to leave.
  /// - Returns: Whether this app is the one.
  static func claim(waiting: TimeInterval = 0) -> Bool {
    try? FileManager.default.createDirectory(
      at: folder,
      withIntermediateDirectories: true,
      attributes: [.posixPermissions: 0o700]
    )
    let lock = open(folder.appending(path: "app.lock").path, O_RDWR | O_CREAT | O_NOFOLLOW, 0o600)
    guard lock >= 0 else { return false }
    let deadline = Date().addingTimeInterval(waiting)
    while flock(lock, LOCK_EX | LOCK_NB) != 0 {
      guard Date() < deadline else {
        close(lock)
        return false
      }
      Thread.sleep(forTimeInterval: 0.25)
    }
    // Held open, and so locked, until the app exits. The record names this very process, its start included, so a
    // later process given the same number is never taken for it; one that can't be written leaves no app running.
    guard let start = startTime() else {
      close(lock)
      return false
    }
    let record: [String: Any] = ["pid": Int(getpid()), "start": [start.seconds, start.microseconds]]
    let path = folder.appending(path: "app.json")
    do {
      let data = try JSONSerialization.data(withJSONObject: record)
      try data.write(to: path, options: .atomic)
      try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: path.path)
    } catch {
      close(lock)
      return false
    }
    return true
  }

  /// When this process started, as the kernel says.
  ///
  /// - Returns: Seconds and microseconds since 1970.
  private static func startTime() -> (seconds: Int, microseconds: Int)? {
    var info = kinfo_proc()
    var size = MemoryLayout<kinfo_proc>.size
    var name: [Int32] = [CTL_KERN, KERN_PROC, KERN_PROC_PID, getpid()]
    guard sysctl(&name, 4, &info, &size, nil, 0) == 0 else { return nil }
    let start = info.kp_proc.p_un.__p_starttime
    return (Int(start.tv_sec), Int(start.tv_usec))
  }
}
