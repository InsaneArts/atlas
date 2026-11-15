import Darwin
import Foundation
import Testing

@testable import AtlasVMKit

@Suite("A Unix listener")
struct UnixListenerTests {
  @Test("a socket left by an earlier run is replaced, anything else in its place is refused, and a peer is taken")
  func listening() throws {
    let folders = try Folders()
    let path = folders.root.appending(path: "control.sock")
    let earlier = try UnixListener(at: path)
    _ = earlier

    let listener = try UnixListener(at: path)
    let client = try #require(Sockets.connect(to: path))
    let taken = try #require(listener.accept())
    Sockets.say("hi", to: client)
    close(client)
    #expect(Sockets.heard(from: taken) == "hi")
    close(taken)

    let file = folders.root.appending(path: "file.sock")
    try Data().write(to: file)
    #expect(throws: UnixListenerError.occupied(file)) { try UnixListener(at: file) }

    var status = stat()
    #expect(lstat(path.path(percentEncoded: false), &status) == 0)
    #expect(status.st_mode & 0o777 == 0o600)
  }

  @Test("a path too long for a socket's address is refused")
  func tooLong() throws {
    let folders = try Folders()
    let long = folders.root.appending(path: String(repeating: "a", count: 120))
    #expect(throws: UnixListenerError.pathTooLong(long)) { try UnixListener(at: long) }
  }
}
