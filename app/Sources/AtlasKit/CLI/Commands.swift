import Foundation

/// The outcome of one `atlas` command: whether it worked, its JSON reply,
/// and what to tell the person in one line.
public struct Outcome: Sendable {
    public let ok: Bool
    /// The reply as the command printed it (JSON), for decoding.
    public let reply: Data
    public let message: String
    /// The failure's kind (`logout-held`, say), when it names one.
    public let code: String?

    public init(ok: Bool, reply: Data, message: String, code: String? = nil) {
        self.ok = ok
        self.reply = reply
        self.message = message
        self.code = code
    }

    /// The reply decoded as `type`, if it is one.
    public func decode<T: Decodable>(_ type: T.Type) -> T? {
        try? JSONDecoder().decode(type, from: reply)
    }

    /// Read a command's output: replies go to stdout, errors to stderr.
    public static func parse(status: Int32, stdout: Data, stderr: Data) -> Outcome {
        struct Reply: Decodable {
            let ok: Bool?
            let error: String?
            let code: String?
        }
        let decoder = JSONDecoder()
        for data in [stdout, stderr] where !data.isEmpty {
            if let reply = try? decoder.decode(Reply.self, from: data) {
                let worked = status == 0 && reply.ok != false
                return Outcome(
                    ok: worked, reply: data, message: worked ? "Done" : (reply.error ?? "It did not work"),
                    code: worked ? nil : reply.code)
            }
        }
        let text = String(decoding: stderr, as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return Outcome(ok: false, reply: Data(), message: text.isEmpty ? "Atlas did not answer" : text)
    }
}

/// Something that runs `atlas` commands; the app's models take this, so
/// tests can stand in for the command line.
public protocol CommandRunning: Sendable {
    func run(_ arguments: [String], input: Data?) async -> Outcome
}

extension CommandRunning {
    public func run(_ arguments: [String]) async -> Outcome {
        await run(arguments, input: nil)
    }
}

/// Runs the `atlas` command line. The app holds no policy of its own: every
/// action is a command, so the command line stays complete.
public struct Commands: CommandRunning {
    public let executable: URL

    public init(executable: URL) {
        self.executable = executable
    }

    /// The `atlas` beside this app's own executable (in Atlas.app).
    public static func beside(_ executable: URL) -> Commands {
        Commands(executable: executable.deletingLastPathComponent().appendingPathComponent("atlas"))
    }

    /// The `atlas` that started this app (`ATLAS_CLI`), else the one beside
    /// it. Its settings arrive as `ATLAS_CONFIG`, which every command inherits.
    public static func parent(fallback executable: URL) -> Commands {
        if let path = ProcessInfo.processInfo.environment["ATLAS_CLI"], !path.isEmpty {
            return Commands(executable: URL(fileURLWithPath: path))
        }
        return beside(executable)
    }

    /// Run `atlas --json arguments…` without blocking the caller. `input`,
    /// if any, is written to the command's standard input: a secret travels
    /// this way, never in its arguments. Cancelling the calling task stops
    /// the command.
    public func run(_ arguments: [String], input: Data?) async -> Outcome {
        let executable = executable
        let running = Running()
        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                DispatchQueue.global(qos: .userInitiated).async {
                    continuation.resume(returning: Self.runNow(executable, arguments, input, running))
                }
            }
        } onCancel: {
            running.stop()
        }
    }

    private static func runNow(
        _ executable: URL, _ arguments: [String], _ input: Data?, _ running: Running
    )
        -> Outcome
    {
        let process = Process()
        process.executableURL = executable
        process.arguments = ["--json"] + arguments
        // What the person does from the app is recorded as theirs, not an agent's.
        process.environment = ProcessInfo.processInfo.environment.merging(["ATLAS_ACTOR": "viewer"]) { $1 }
        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr
        let stdin = input.map { _ in Pipe() }
        process.standardInput = stdin ?? FileHandle.nullDevice
        do {
            guard try running.start(process) else {
                return Outcome(ok: false, reply: Data(), message: "Cancelled")
            }
        } catch {
            return Outcome(ok: false, reply: Data(), message: error.localizedDescription)
        }
        if let stdin, let input {
            // A command that exits early closes the pipe: the write fails
            // with an error, caught here, instead of raising SIGPIPE.
            _ = fcntl(stdin.fileHandleForWriting.fileDescriptor, F_SETNOSIGPIPE, 1)
            try? stdin.fileHandleForWriting.write(contentsOf: input)
            try? stdin.fileHandleForWriting.close()
        }
        // Both outputs are read at once, so neither can fill and stall the other.
        let errors = Errors()
        let reading = DispatchGroup()
        reading.enter()
        DispatchQueue.global(qos: .userInitiated).async {
            errors.data = stderr.fileHandleForReading.readDataToEndOfFile()
            reading.leave()
        }
        let out = stdout.fileHandleForReading.readDataToEndOfFile()
        reading.wait()
        process.waitUntilExit()
        return Outcome.parse(status: process.terminationStatus, stdout: out, stderr: errors.data)
    }

    /// A command that may be stopped from another task: started at most
    /// once, and never after it was stopped.
    private final class Running: @unchecked Sendable {
        private let lock = NSLock()
        private var process: Process?
        private var stopped = false

        /// Start `process`, unless already stopped; whether it runs.
        func start(_ process: Process) throws -> Bool {
            try lock.withLock {
                guard !stopped else { return false }
                try process.run()
                self.process = process
                return true
            }
        }

        func stop() {
            lock.withLock {
                stopped = true
                if process?.isRunning == true { process?.terminate() }
            }
        }
    }

    /// The standard error collected on another thread.
    private final class Errors: @unchecked Sendable {
        // Written once before `reading.leave()`, read once after `wait()`.
        var data = Data()
    }
}

/// What the panel's actions run.
public enum Action: Sendable, Equatable {
    /// Lock the screen to switch to this account (the helper, or back).
    case switchTo(String)
    case recover
    case logOut(force: Bool)

    public var arguments: [String] {
        switch self {
        // The person clicked Switch: that is the confirmation.
        case .switchTo(let account): ["switch", account, "--yes"]
        case .recover: ["restart"]
        // The person confirmed in the panel.
        case .logOut(let force): ["logout", "--yes"] + (force ? ["--force"] : [])
        }
    }
}
