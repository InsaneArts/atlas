import Darwin

/// What the kernel says of a process: running since a moment, gone, or not
/// said.
public enum ProcessLife: Equatable, Sendable {
  /// It runs, and started at `seconds` and `microseconds` past the epoch.
  case running(seconds: Int, microseconds: Int)
  /// No process has its PID, or it ended and waits to be reaped.
  case gone
  /// The kernel did not say; never taken for gone.
  case unknown

  /// What the kernel says of the process `pid` now.
  public static func of(_ pid: Int32) -> ProcessLife {
    var info = kinfo_proc()
    var size = MemoryLayout<kinfo_proc>.stride
    var name: [Int32] = [CTL_KERN, KERN_PROC, KERN_PROC_PID, pid]
    guard sysctl(&name, UInt32(name.count), &info, &size, nil, 0) == 0 else { return .unknown }
    guard size > 0, info.kp_proc.p_stat != SZOMB else { return .gone }
    let started = info.kp_proc.p_starttime
    return .running(seconds: Int(started.tv_sec), microseconds: Int(started.tv_usec))
  }
}
