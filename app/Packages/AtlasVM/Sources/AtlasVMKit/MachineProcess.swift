import Darwin

/// The process of Apple's framework that runs one VM, told from any later
/// process that reuses its PID by when it started.
public struct MachineProcess: Codable, Equatable, Sendable {
  /// Its process's ID.
  public let pid: Int32
  /// When it started, in seconds past the epoch.
  public let startSeconds: Int
  /// When it started, the microseconds past `startSeconds`.
  public let startMicroseconds: Int

  /// The name of the framework's program that runs a VM.
  private static let program = "/com.apple.Virtualization.VirtualMachine"

  /// The process `pid` as it is now; none unless it runs.
  public init?(pid: Int32) {
    guard case .running(let seconds, let microseconds) = ProcessLife.of(pid) else { return nil }
    self.pid = pid
    startSeconds = seconds
    startMicroseconds = microseconds
  }

  /// Whether it has exited: its PID gone, or another process's. None when
  /// the kernel does not say, which is never gone.
  public func exited() -> Bool? {
    switch ProcessLife.of(pid) {
    case .gone:
      true
    case .running(let seconds, let microseconds):
      (seconds, microseconds) != (startSeconds, startMicroseconds)
    case .unknown:
      nil
    }
  }

  /// Every process of the framework's that runs a VM now, for this
  /// account; none when that cannot be told whole: a listing that may be
  /// cut short, or one of this account's processes whose program cannot be
  /// read. A VM's process is always this account's.
  public static func running() -> [MachineProcess]? {
    let expected = proc_listallpids(nil, 0)
    guard expected > 0 else { return nil }
    var pids = [Int32](repeating: 0, count: Int(expected) * 2 + 64)
    let listed = pids.withUnsafeMutableBytes { bytes in
      proc_listallpids(bytes.baseAddress, Int32(bytes.count))
    }
    // A full buffer may have left some out.
    guard listed > 0, Int(listed) < pids.count else { return nil }
    var found: [MachineProcess] = []
    for pid in pids.prefix(Int(listed)) {
      switch Self.examine(pid) {
      case .none:
        return nil
      case .some(.none):
        continue
      case .some(.some(let machine)):
        found.append(machine)
      }
    }
    return found
  }

  /// What `pid` is: a VM's process of this account's, not one (another
  /// program, another account's, or gone), or none when that cannot be told.
  private static func examine(_ pid: Int32) -> MachineProcess?? {
    var info = kinfo_proc()
    var size = MemoryLayout<kinfo_proc>.stride
    var name: [Int32] = [CTL_KERN, KERN_PROC, KERN_PROC_PID, pid]
    guard sysctl(&name, UInt32(name.count), &info, &size, nil, 0) == 0 else { return nil }
    guard size > 0, info.kp_proc.p_stat != SZOMB, info.kp_eproc.e_ucred.cr_uid == getuid() else {
      return .some(nil)
    }
    var path = [CChar](repeating: 0, count: Int(MAXPATHLEN) * 4)
    guard proc_pidpath(pid, &path, UInt32(path.count)) > 0 else {
      // Gone meanwhile is not one; nor is one whose program was deleted since
      // it started, as the framework's, on the sealed system volume, never
      // is, unless its short name says it may be. Anything else cannot be told.
      let deleted = errno == ENOENT
      let short = withUnsafeBytes(of: info.kp_proc.p_comm) { bytes in
        String(decoding: bytes.prefix { $0 != 0 }, as: UTF8.self)
      }
      if ProcessLife.of(pid) == .gone || (deleted && !program.dropFirst().hasPrefix(short)) {
        return .some(nil)
      }
      return nil
    }
    guard String(cString: path).hasSuffix(program) else { return .some(nil) }
    // One gone meanwhile is not one; one the kernel will not say cannot be told.
    if let machine = MachineProcess(pid: pid) { return .some(machine) }
    return ProcessLife.of(pid) == .gone ? .some(nil) : nil
  }
}
