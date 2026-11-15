import Darwin
import Foundation
import Testing

@testable import AtlasVMKit

@Suite("The line server")
struct LineServerTests {
  @Test("each line is answered in turn, and a line too long ends the connection unanswered")
  func lines() async throws {
    let (client, server) = Sockets.pair()
    Sockets.say("one\ntwo\n" + String(repeating: "x", count: 100), to: client)
    shutdown(client, SHUT_WR)

    await LineServer.serve(server, limit: 50) { line in
      Data("got \(String(decoding: line, as: UTF8.self))\n".utf8)
    }

    #expect(Sockets.heard(from: client) == "got one\ngot two\n")
    close(client)
  }
}
