import AtlasService
import Foundation
import Testing

@testable import AtlasApp

/// Against the menu bar running on this Mac, only when `ATLAS_PIPE_LIVE` is set: the app's own lanes, observer and
/// decoding stream pictures from the frame service.
@Suite("Native frame pipe, live", .enabled(if: ProcessInfo.processInfo.environment["ATLAS_PIPE_LIVE"] != nil))
struct NativeFramePipeLiveTests {
  @Test("the app's own lanes stream, decode and commit live pictures")
  func streams() async throws {
    let record = FileManager.default.homeDirectoryForCurrentUser
      .appending(path: "Library/Application Support/Atlas/runs/menubar.json")
    let object = try JSONSerialization.jsonObject(with: Data(contentsOf: record)) as? [String: Any]
    let url = try #require(object?["url"] as? String)
    let service = try #require(ViewerClient(url))
    let descriptor = try #require(try await service.framePipe())
    let lanes = try await NativeFramePipeConnector().connect(descriptor)
    let client = FrameStreamClient(descriptor: descriptor, control: lanes.control, frame: lanes.frame)
    try await client.open(longest: ViewerFramePump.longest)
    let began = ContinuousClock.now
    var ages: [Double] = []
    let count = 90
    var last: FramePipePicture?
    for _ in 0..<count {
      let picture = try await client.next(longest: ViewerFramePump.longest)
      let decoded = try #require(await ViewerFramePump.decode(picture))
      #expect(decoded.picture.image.width == picture.width)
      ages.append(picture.ageAtHeader)
      try await client.commit(picture)
      last = picture
    }
    // A committed picture is one control may be taken on, through the viewer's own guarded route.
    let committed = try #require(last)
    let lease = try await service.acquire(frame: committed.frameKey)
    try await service.release(lease: lease)
    let seconds = (ContinuousClock.now - began) / .seconds(1)
    await client.close()
    ages.sort()
    print(
      "frame-pipe-live fps=\(Double(count) / seconds) ageP50=\(ages[ages.count / 2]) ageP95=\(ages[ages.count * 95 / 100])"
    )
    #expect(Double(count) / seconds > 10)
  }
}
