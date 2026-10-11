import Foundation
import XCTest

@testable import AtlasKit

final class SnapshotTests: XCTestCase {
    func testStatusReportsDecodeIntoWhatThePanelShows() {
        let status: [String: Any] = [
            "overview": [
                "mode": "controlling",
                "account": "atlastest",
                "display": ["width": 2560.0, "height": 1440.0],
                "activity": [
                    ["operation": "input", "actor": "viewer", "ok": true, "actionCount": 3, "ageSeconds": 42.0]
                ],
            ]
        ]
        let snapshot = Snapshot(status: status)
        XCTAssertEqual(snapshot.mode, "controlling")
        XCTAssertTrue(snapshot.controlling)
        XCTAssertEqual(snapshot.display, "2560 × 1440")
        XCTAssertEqual(snapshot.last, "You sent 3 actions")
        XCTAssertEqual(snapshot.when, "42 s ago")
        XCTAssertEqual(Snapshot(status: [:]).mode, "unavailable")
    }

    func testOperationsAndAgesReadAsWords() {
        XCTAssertEqual(Snapshot.describe("run", ["ok": true]), "ran a command")
        XCTAssertEqual(Snapshot.describe("input", ["ok": false]), "input failed")
        XCTAssertEqual(Snapshot.describe("input", ["ok": true, "actionCount": 1]), "sent 1 action")
        XCTAssertEqual(Snapshot.ago(2), "Now")
        XCTAssertEqual(Snapshot.ago(600), "10 min ago")
        XCTAssertEqual(Snapshot.ago(7200), "2 h ago")
    }

    func testEveryModeHasALabelToneAndMark() {
        XCTAssertEqual(Mode.of("observing").tone, .ready)
        XCTAssertEqual(Mode.of("controlling").mark, .filled)
        XCTAssertEqual(Mode.of("recovering").tone, .attention)
        XCTAssertEqual(Mode.of("anything else").label, "Unavailable")
    }
}

final class ViewerTests: XCTestCase {
    func testOnlyALocalCapabilityURLIsAccepted() {
        XCTAssertNotNil(Viewer("http://127.0.0.1:8765/abc/\n"))
        XCTAssertNil(Viewer("http://example.com:8765/abc/"))
        XCTAssertNil(Viewer("https://127.0.0.1:8765/abc/"))
        XCTAssertNil(Viewer("http://127.0.0.1/abc/"))
        XCTAssertNil(Viewer("http://127.0.0.1:8765/abc"))
    }
}

final class CommandTests: XCTestCase {
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

    func outcome(_ commands: Commands, _ arguments: [String]) -> Outcome {
        let done = expectation(description: "ran")
        nonisolated(unsafe) var result: Outcome?
        commands.run(arguments) {
            result = $0
            done.fulfill()
        }
        wait(for: [done], timeout: 10)
        return result!
    }

    func testRepliesAndErrorsAreReadFromTheRightStream() throws {
        let ok = try fake("echo '{\"ok\": true, \"args\": \"'\"$*\"'\"}'")
        let reply = outcome(ok, ["switch", "--list"])
        XCTAssertTrue(reply.ok)
        XCTAssertEqual(reply.reply["args"] as? String, "--json switch --list")
        let refused = try fake("echo '{\"ok\": false, \"error\": \"busy\"}' >&2; exit 1")
        let failure = outcome(refused, ["restart"])
        XCTAssertFalse(failure.ok)
        XCTAssertEqual(failure.message, "busy")
        let silent = try fake("echo 'not json' >&2; exit 3")
        XCTAssertEqual(outcome(silent, []).message, "not json")
    }

    func testActionsMapToCommands() {
        XCTAssertEqual(Action.switchToHelper("atlastest").arguments, ["switch", "atlastest", "--yes"])
        XCTAssertEqual(Action.recover.arguments, ["restart"])
        XCTAssertEqual(Commands.beside(URL(fileURLWithPath: "/A.app/Contents/MacOS/AtlasBar")).executable.path,
            "/A.app/Contents/MacOS/atlas")
    }
}
