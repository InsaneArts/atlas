import Foundation

@testable import AtlasCommands

/// Runs `body` with a stand-in `atlas` that runs `script`, printing a reply and exiting with a status, then deletes
/// the temporary folder it lives in.
///
/// - Parameters:
///   - script: The shell script `atlas` runs.
///   - environment: The environment the app hands on to `atlas`.
///   - body: What the test does with the command line.
/// - Returns: What `body` returns.
/// - Throws: When the folder cannot be made, or what `body` throws.
func withScriptedAtlas<Result>(
  _ script: String,
  environment: [String: String] = [:],
  _ body: (Commands) async throws -> Result
) async throws -> Result {
  let files = FileManager.default
  let directory = files.temporaryDirectory
    .appendingPathComponent("atlas-kit-\(UUID().uuidString)")
  try files.createDirectory(at: directory, withIntermediateDirectories: true)
  defer { try? files.removeItem(at: directory) }
  let file = directory.appendingPathComponent("atlas")
  let contents = "#!/bin/sh\n" + script
  try contents.write(to: file, atomically: true, encoding: .utf8)
  try files.setAttributes([.posixPermissions: 0o755], ofItemAtPath: file.path)
  return try await body(Commands(executable: file, environment: environment))
}
