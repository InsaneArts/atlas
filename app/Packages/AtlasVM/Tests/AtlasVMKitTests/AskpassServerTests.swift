import Foundation
import Testing

@testable import AtlasVMKit

@Suite("A sign-in's password server")
struct AskpassServerTests {
  @Test("stopped, whether before or while it serves, it ends though no helper ever asked, and gives nothing")
  func stopped() async throws {
    let folders = try Folders()
    let server = try AskpassServer(at: folders.root.appending(path: "askpass"))

    let serving = Task.detached { server.serve("the password", toChildOf: getpid()) }
    server.stop()

    #expect(await serving.value == false)
  }
}
