import Foundation

/// The outcome of one `atlas` command: its JSON reply, or why there was
/// none.
public struct Outcome: @unchecked Sendable {
    public let ok: Bool
    public let reply: [String: Any]
    /// What to tell the person, in one line.
    public let message: String

    public init(ok: Bool, reply: [String: Any], message: String) {
        self.ok = ok
        self.reply = reply
        self.message = message
    }

    /// Read a command's output: replies go to stdout, errors to stderr.
    public static func parse(status: Int32, stdout: Data, stderr: Data) -> Outcome {
        let decode = { (data: Data) in
            (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
        }
        guard let reply = decode(stdout) ?? decode(stderr) else {
            let text = String(data: stderr, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines)
            return Outcome(
                ok: false, reply: [:],
                message: text.flatMap { $0.isEmpty ? nil : $0 } ?? "Atlas did not answer")
        }
        let ok = status == 0 && reply["ok"] as? Bool != false
        let message = ok ? "Done" : (reply["error"] as? String ?? "It did not work")
        return Outcome(ok: ok, reply: reply, message: message)
    }
}

/// Runs the `atlas` command line. The menu bar holds no policy of its
/// own: every action is a CLI command, so the CLI stays complete.
public struct Commands: Sendable {
    public let executable: URL

    public init(executable: URL) {
        self.executable = executable
    }

    /// The `atlas` beside this app's own executable (in Atlas.app).
    public static func beside(_ executable: URL) -> Commands {
        Commands(executable: executable.deletingLastPathComponent().appendingPathComponent("atlas"))
    }

    /// The `atlas` that started this app (`ATLAS_CLI`), else the
    /// one beside it. Its configuration arrives as `ATLAS_CONFIG`, which
    /// every command inherits.
    public static func parent(fallback executable: URL) -> Commands {
        if let path = ProcessInfo.processInfo.environment["ATLAS_CLI"], !path.isEmpty {
            return Commands(executable: URL(fileURLWithPath: path))
        }
        return beside(executable)
    }

    /// Run `atlas arguments…` off the main thread; the outcome arrives
    /// on the main queue.
    public func run(_ arguments: [String], _ done: @escaping @MainActor @Sendable (Outcome) -> Void) {
        let executable = self.executable
        DispatchQueue.global(qos: .userInitiated).async {
            let process = Process()
            process.executableURL = executable
            // Replies are always JSON, whatever the outputs are attached to.
            process.arguments = ["--json"] + arguments
            let stdout = Pipe()
            let stderr = Pipe()
            process.standardOutput = stdout
            process.standardError = stderr
            process.standardInput = FileHandle.nullDevice
            let outcome: Outcome
            do {
                try process.run()
                let out = stdout.fileHandleForReading.readDataToEndOfFile()
                let err = stderr.fileHandleForReading.readDataToEndOfFile()
                process.waitUntilExit()
                outcome = Outcome.parse(status: process.terminationStatus, stdout: out, stderr: err)
            } catch {
                outcome = Outcome(ok: false, reply: [:], message: error.localizedDescription)
            }
            Task { @MainActor in done(outcome) }
        }
    }
}

/// What the panel's actions run.
public enum Action: Sendable {
    case switchToHelper(String)
    case recover
    case doctor

    public var arguments: [String] {
        switch self {
        // The person clicked Switch: that is the confirmation.
        case .switchToHelper(let account): return ["switch", account, "--yes"]
        case .recover: return ["restart"]
        case .doctor: return ["doctor"]
        }
    }
}
