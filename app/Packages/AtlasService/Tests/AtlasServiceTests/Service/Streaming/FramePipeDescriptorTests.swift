import Foundation
import Testing

@testable import AtlasService

@Suite("Frame pipe descriptors")
struct FramePipeDescriptorTests {
  @Test("a frame pipe descriptor without a helper account decodes")
  func withoutHelper() throws {
    let service = String(repeating: "a", count: 32)
    let bootstrap = String(repeating: "b", count: 64)
    let said = """
      {"recordVersion": 1, "protocolMajor": 1, "controllerUID": 501, "brokerPID": 42,
       "serviceInstance": "\(service)", "observerBootstrap": "\(bootstrap)",
       "controlPath": "/private/tmp/atlas-fp.501.0123456789abcdef/c.sock",
       "framePath": "/private/tmp/atlas-fp.501.0123456789abcdef/f.sock",
       "desktopKey": "vm-0123456789abcdef01234567"}
      """
    let descriptor = try JSONDecoder()
      .decode(FramePipeDescriptor.self, from: Data(said.utf8))
    #expect(
      descriptor
        == FramePipeDescriptor(
          controlPath: "/private/tmp/atlas-fp.501.0123456789abcdef/c.sock",
          framePath: "/private/tmp/atlas-fp.501.0123456789abcdef/f.sock",
          serviceInstance: service,
          observerBootstrap: bootstrap,
          brokerPID: 42,
          controllerUID: 501,
          desktopKey: "vm-0123456789abcdef01234567"
        )
    )
  }
}
