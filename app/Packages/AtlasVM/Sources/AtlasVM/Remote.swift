import AtlasVMKit
import Foundation

/// One command run in the guest over Remote Login: its host key pinned, its
/// password given to `ssh`'s helper alone, never in an argument, a file or
/// the environment.
struct Remote {
  /// The guest account it signs in as.
  let user: String
  /// The guest's address.
  let address: String
  /// That account's password.
  let password: String
  /// The VM's folder, where its host key is pinned and its helper's socket made.
  let folder: VMFolder

  /// How long one command may take.
  private static let longest: TimeInterval = 600

  /// Run `command` in the guest, `input` on its standard input: what it
  /// printed, when it succeeded.
  func run(_ command: String, input: Data = Data()) throws -> Data {
    let socket = folder.root.appending(path: "askpass-\(UUID().uuidString.prefix(8))")
    let server = try AskpassServer(at: socket)
    defer {
      try? FileManager.default
        .removeItem(at: socket)
    }
    let hostKey =
      folder.root
      .appending(path: "HostKey")
      .path(percentEncoded: false)
    let pinned = FileManager.default
      .fileExists(atPath: hostKey)
    let ssh = Process()
    ssh.executableURL = URL(filePath: "/usr/bin/ssh")
    ssh.arguments = [
      // Quoted, as `ssh` splits this option at spaces, and its `%` tokens
      // escaped: the VM's folder is under Application Support.
      "-o", "UserKnownHostsFile=\"\(hostKey.replacingOccurrences(of: "%", with: "%%"))\"",
      "-o", "StrictHostKeyChecking=\(pinned ? "yes" : "accept-new")",
      "-o", "PubkeyAuthentication=no",
      "-o", "PreferredAuthentications=keyboard-interactive,password",
      "-o", "NumberOfPasswordPrompts=1",
      "-o", "ConnectTimeout=15",
      "\(user)@\(address)", command,
    ]
    var environment = ProcessInfo.processInfo.environment
    environment["SSH_ASKPASS"] = Bundle.main.executableURL?
      .path(percentEncoded: false)
    environment["SSH_ASKPASS_REQUIRE"] = "force"
    environment["DISPLAY"] = "none"
    environment["ATLAS_VM_ASKPASS"] = socket.path(percentEncoded: false)
    ssh.environment = environment
    let (standardInput, standardOutput) = (Pipe(), Pipe())
    ssh.standardInput = standardInput
    ssh.standardOutput = standardOutput
    ssh.standardError = FileHandle.nullDevice
    try ssh.run()
    let pid = ssh.processIdentifier
    let started = ContinuousClock.now
    // Ended at its deadline whatever it is doing, so no read or write below
    // waits on a guest that stopped answering.
    let deadline = DispatchWorkItem { kill(pid, SIGTERM) }
    DispatchQueue.global()
      .asyncAfter(deadline: .now() + Self.longest, execute: deadline)
    let helpers = DispatchGroup()
    let password = password
    helpers.enter()
    Thread.detachNewThread {
      _ = server.serve(password, toChildOf: pid)
      helpers.leave()
    }
    let writing = standardInput.fileHandleForWriting
    helpers.enter()
    Thread.detachNewThread {
      try? writing.write(contentsOf: input)
      try? writing.close()
      helpers.leave()
    }
    let printed = standardOutput.fileHandleForReading.readDataToEndOfFile()
    ssh.waitUntilExit()
    deadline.cancel()
    // A sign-in that never asked leaves its helper's server waiting: ended.
    server.stop()
    helpers.wait()
    if ContinuousClock.now - started >= .seconds(Self.longest) {
      throw RemoteFailure(description: "The guest did not answer in time.")
    }
    guard ssh.terminationStatus == 0 else {
      throw RemoteFailure(
        description: "A command in the guest failed: \(String(decoding: printed.prefix(400), as: UTF8.self))",
        printed: printed,
        // `ssh` exits 255 for its own failures, such as a refused connection.
        unreachable: ssh.terminationStatus == 255 && printed.isEmpty
      )
    }
    return printed
  }

  /// Run the Atlas command `command` as root through `sudo`, which asks this
  /// account's password every time and takes it as the first line of
  /// standard input; `payload`, when given, is the second.
  func sudo(_ command: String, payload: String? = nil) throws -> Data {
    var input = password + "\n"
    if let payload { input += payload + "\n" }
    return try run("sudo -k -S -p '' \(command)", input: Data(input.utf8))
  }
}
