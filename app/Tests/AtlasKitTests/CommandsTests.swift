import Foundation
import Testing

@testable import AtlasKit

@Suite struct CommandTests {
    /// A stand-in `atlas` that prints a reply and an exit status.
    func fake(_ script: String) throws -> Commands {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("atlas-kit-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let file = directory.appendingPathComponent("atlas")
        try ("#!/bin/sh\n" + script).write(to: file, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: file.path)
        return Commands(executable: file)
    }

    @Test func repliesAndErrorsAreReadFromTheRightStream() async throws {
        struct Echo: Decodable { let args: String }
        let reply = try await fake("echo '{\"ok\": true, \"args\": \"'\"$*\"'\"}'").run(["switch", "--list"])
        #expect(reply.ok)
        #expect(reply.decode(Echo.self)?.args == "--json switch --list")
        let failure = try await fake("echo '{\"ok\": false, \"error\": \"busy\"}' >&2; exit 1").run(["restart"])
        #expect(!failure.ok)
        #expect(failure.message == "busy")
        #expect(try await fake("echo 'not json' >&2; exit 3").run([]).message == "not json")
    }

    @Test func inputArrivesOnStandardInputNeverInArguments() async throws {
        struct Echo: Decodable { let got: String }
        let reply = try await fake("read line; echo '{\"ok\": true, \"got\": \"'\"$line\"'\"}'")
            .run(["onboarding", "create"], input: Data("secret\n".utf8))
        #expect(reply.decode(Echo.self)?.got == "secret")
    }

    @Test func aCommandThatIgnoresItsInputStillFinishes() async throws {
        let reply = try await fake("echo '{\"ok\": true}'").run([], input: Data(repeating: 0x41, count: 1 << 20))
        #expect(reply.ok)
    }

    @Test func cancellingTheCallerStopsTheCommand() async throws {
        let commands = try fake("sleep 30; echo '{\"ok\": true}'")
        let started = ContinuousClock.now
        let running = Task { await commands.run(["status"]) }
        try await Task.sleep(for: .milliseconds(300))
        running.cancel()
        let outcome = await running.value
        #expect(!outcome.ok)
        #expect(ContinuousClock.now - started < .seconds(10))
    }

    @Test func actionsMapToCommands() {
        #expect(Action.switchToHelper("atlastest").arguments == ["switch", "atlastest", "--yes"])
        #expect(Action.recover.arguments == ["restart"])
        #expect(
            Commands.beside(URL(fileURLWithPath: "/A.app/Contents/MacOS/AtlasApp")).executable.path
                == "/A.app/Contents/MacOS/atlas")
    }
}
