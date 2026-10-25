import AsyncSupportTesting
import Foundation
import Testing

@testable import AtlasCommands

@Suite("Commands")
struct CommandsTests {
  @Test("a reply is read from standard output, after --json and the arguments")
  func aReplyIsReadFromStandardOutput() async throws {
    struct Echo: Decodable { let args: String }
    let reply = try await withScriptedAtlas(#"echo '{"ok": true, "args": "'"$*"'"}'"#) {
      await $0.run(["switch", "--list"])
    }
    #expect(reply.ok)
    #expect(reply.decode(Echo.self)?.args == "--json switch --list")
  }

  @Test("an error is read from standard error, as its JSON says it")
  func anErrorIsReadFromStandardError() async throws {
    let failure = try await withScriptedAtlas(#"echo '{"ok": false, "error": "busy"}' >&2; exit 1"#) {
      await $0.run(["restart"])
    }
    #expect(!failure.ok)
    #expect(failure.message == "busy")
  }

  @Test("an error that is not JSON is told as it was printed")
  func aPlainErrorIsToldAsPrinted() async throws {
    let failure = try await withScriptedAtlas("echo 'not json' >&2; exit 3") { await $0.run([]) }
    #expect(failure.message == "not json")
  }

  @Test("every command runs as the person's, with ATLAS_ACTOR=viewer over the app's own environment")
  func everyCommandIsThePersons() async throws {
    struct Echo: Decodable {
      let actor: String
      let config: String
    }
    let reply = try await withScriptedAtlas(
      #"echo '{"ok": true, "actor": "'"$ATLAS_ACTOR"'", "config": "'"$ATLAS_CONFIG"'"}'"#,
      environment: ["ATLAS_ACTOR": "agent", "ATLAS_CONFIG": "/tmp/atlas.toml"]
    ) { await $0.run(["status"]) }
    #expect(reply.decode(Echo.self)?.actor == "viewer")
    #expect(reply.decode(Echo.self)?.config == "/tmp/atlas.toml", "the rest of the environment is handed on")
  }

  @Test("a command never inherits the login job's name launchd gave the app, so atlas never takes itself for it")
  func noCommandIsTheLoginJob() async throws {
    struct Echo: Decodable { let job: String }
    let reply = try await withScriptedAtlas(
      #"echo '{"ok": true, "job": "'"${XPC_SERVICE_NAME-none}"'"}'"#,
      environment: ["XPC_SERVICE_NAME": "com.insanearts.atlas.menubar"]
    ) { await $0.run(["menubar"]) }
    #expect(reply.decode(Echo.self)?.job == "none")
  }

  @Test("a command run now, before any app, runs its action's arguments as the person's too")
  func runNowIsThePersons() async throws {
    struct Echo: Decodable {
      let actor: String
      let args: String
    }
    let reply = try await withScriptedAtlas(#"echo '{"ok": true, "actor": "'"$ATLAS_ACTOR"'", "args": "'"$*"'"}'"#) {
      $0.runNow(.lockScreen)
    }
    #expect(reply.decode(Echo.self)?.actor == "viewer")
    #expect(reply.decode(Echo.self)?.args == "--json " + Action.lockScreen.arguments.joined(separator: " "))
  }

  @Test("a secret reaches the command on standard input, never in its arguments")
  func secretOnStandardInput() async throws {
    struct Echo: Decodable { let got: String }
    let reply = try await withScriptedAtlas(#"read line; echo '{"ok": true, "got": "'"$line"'"}'"#) {
      await $0.run(["onboarding", "create"], input: Data("secret\n".utf8))
    }
    #expect(reply.decode(Echo.self)?.got == "secret")
  }

  @Test("a command that never reads its input still finishes")
  func unreadInput() async throws {
    let reply = try await withScriptedAtlas(#"echo '{"ok": true}'"#) {
      await $0.run([], input: Data(repeating: 0x41, count: 1 << 20))
    }
    #expect(reply.ok)
  }

  @Test("cancelling the caller stops the command")
  func cancellation() async throws {
    // The command says it has started through a pipe beside itself, then becomes a sleep far longer than the ceiling:
    // `exec`, so stopping it stops the sleep too, which no shell is left holding the output for.
    let outcome = try await withScriptedAtlas(#"echo started > "$(dirname "$0")/started"; exec sleep 30"#) {
      commands in
      let started = commands.executable
        .deletingLastPathComponent()
        .appendingPathComponent("started")
      try #require(mkfifo(started.path, 0o600) == 0, "the pipe could not be made")
      let running = Task { await commands.run(["status"]) }
      // Unstructured, so the ceiling bounds the read.
      let said = Task {
        await withCheckedContinuation { continuation in
          // A thread of its own: opening the pipe blocks until the command opens its end, which is the sentinel.
          Thread.detachNewThread {
            continuation.resume(returning: FileHandle(forReadingAtPath: started.path)?.readDataToEndOfFile())
          }
        }
      }
      #expect(try await said.value("the command never started") == Data("started\n".utf8))
      running.cancel()
      return try await running.value("the cancelled command never ended")
    }
    #expect(!outcome.ok)
  }

  @Test("the atlas beside the app's executable is the one it runs")
  func besideTheApp() {
    #expect(
      Commands.beside(URL(fileURLWithPath: "/A.app/Contents/MacOS/AtlasApp"), environment: [:]).executable.path
        == "/A.app/Contents/MacOS/atlas"
    )
  }
}
