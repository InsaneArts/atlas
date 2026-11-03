import Darwin

/// Who the kernel says serves a connected Unix socket: never what the peer says of itself.
nonisolated enum NativeSocketPeer {
  /// Whether `socket`'s peer is the process `pid` of account `uid`.
  ///
  /// - Parameters:
  ///   - socket: A connected Unix stream socket.
  ///   - pid: The process that must serve it.
  ///   - uid: The account that must serve it.
  /// - Returns: Whether the kernel names exactly that process and account.
  static func isServed(_ socket: Int32, by pid: Int32, of uid: uid_t) -> Bool {
    var account: uid_t = 0
    var group: gid_t = 0
    guard getpeereid(socket, &account, &group) == 0, account == uid else { return false }
    var peer: pid_t = 0
    var length = socklen_t(MemoryLayout<pid_t>.size)
    guard getsockopt(socket, SOL_LOCAL, LOCAL_PEERPID, &peer, &length) == 0,
      length == socklen_t(MemoryLayout<pid_t>.size)
    else { return false }
    return peer == pid
  }
}
