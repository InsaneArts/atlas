import AtlasService
import Foundation

extension Commands {
  /// Run `atlas --json arguments…` to its end, never stopping it: its standard output and error go to two private
  /// files in `folder`, read back and removed once it ends. Cancelling the calling task ends only the wait; the
  /// command goes on, and its files stay for as long as it runs.
  ///
  /// - Parameters:
  ///   - arguments: The command's arguments, after `--json`.
  ///   - folder: Where its outputs are kept while it runs, made private to this account if it is not there.
  /// - Returns: The command's outcome; one that could not start, or whose wait was cancelled, failed.
  public func runLasting(_ arguments: [String], keeping folder: URL) async -> Outcome {
    let manager = FileManager.default
    let token = UUID().uuidString
    let output = folder.appending(path: "\(token).json")
    let errors = folder.appending(path: "\(token).log")
    let process = Process()
    let handles: [FileHandle]
    do {
      try manager.createDirectory(at: folder, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
      for file in [output, errors] {
        guard manager.createFile(atPath: file.path, contents: nil, attributes: [.posixPermissions: 0o600]) else {
          throw CocoaError(.fileWriteNoPermission)
        }
      }
      process.executableURL = executable
      process.arguments = ["--json"] + arguments
      process.environment = environment
      process.standardInput = FileHandle.nullDevice
      handles = [try FileHandle(forWritingTo: output), try FileHandle(forWritingTo: errors)]
      process.standardOutput = handles[0]
      process.standardError = handles[1]
    } catch {
      try? manager.removeItem(at: output)
      try? manager.removeItem(at: errors)
      return Outcome(ok: false, reply: Data(), message: error.localizedDescription)
    }
    let wait = LastingWait()
    process.terminationHandler = { ended in
      let stdout = (try? Data(contentsOf: output)) ?? Data()
      let stderr = (try? Data(contentsOf: errors)) ?? Data()
      let files = FileManager.default
      try? files.removeItem(at: output)
      try? files.removeItem(at: errors)
      wait.end(Outcome.parse(status: ended.terminationStatus, stdout: stdout, stderr: stderr))
    }
    return await withTaskCancellationHandler {
      await withCheckedContinuation { continuation in
        wait.hold(continuation)
        do {
          try process.run()
          // The command holds its own copies; this app's are not needed.
          for handle in handles { try? handle.close() }
        } catch {
          wait.end(Outcome(ok: false, reply: Data(), message: error.localizedDescription))
        }
        if Task.isCancelled { wait.end(LastingWait.givenUp) }
      }
    } onCancel: {
      wait.end(LastingWait.givenUp)
    }
  }
}
