import Foundation
import Testing

@testable import AtlasVMKit

@Suite("How far making a VM got")
struct MakingLogTests {
  let change = ChangeID.repeating("5e")

  @Test("download progress is kept at most once a second and survives a resumed download")
  func keptOnceASecond() throws {
    let folders = try Folders()
    let file = try folders.folder("vm")
      .appending(path: "Making.json")
    let clock = ManualClock()
    let log = MakingLog(file: file, change: change) { clock.now }

    log.checking()
    log.downloading(received: 100, total: 1000)
    let first = try MakingProgress.read(from: file)
    clock.advance(by: .milliseconds(500))
    log.downloading(received: 200, total: 1000)
    let within = try MakingProgress.read(from: file)
    clock.advance(by: .milliseconds(600))
    log.downloading(received: 300, total: 1000)
    let after = try MakingProgress.read(from: file)

    #expect(first?.phase == .downloading && first?.received == 100 && first?.total == 1000)
    #expect(within == first, "not written again within a second")
    #expect(after?.received == 300 && after?.fraction == 0.3)

    // A later run goes on with the same make, its download resumed: what
    // the earlier tries received is never said lost.
    let resumed = MakingLog(file: file, change: change) { clock.now }
    resumed.checking()
    clock.advance(by: .seconds(1))
    resumed.downloading(received: 40, total: 1000)
    let again = try MakingProgress.read(from: file)
    clock.advance(by: .seconds(1))
    resumed.downloading(received: 450, total: 1000)
    let on = try MakingProgress.read(from: file)
    resumed.installing(fraction: 0.1)
    let installing = try MakingProgress.read(from: file)

    #expect(again?.phase == .downloading && again?.received == 300)
    #expect(on?.received == 450 && on?.change == change)
    #expect(installing?.phase == .installing && installing?.fraction == 0.1 && installing?.received == nil)
  }

  @Test("another make's progress is never counted on from")
  func anotherMake() throws {
    let folders = try Folders()
    let file = try folders.folder("vm")
      .appending(path: "Making.json")
    let clock = ManualClock()
    MakingLog(file: file, change: change) { clock.now }
      .downloading(received: 900, total: 1000)

    let other = ChangeID.repeating("6f")
    MakingLog(file: file, change: other) { clock.now }
      .downloading(received: 10, total: 1000)

    let kept = try MakingProgress.read(from: file)
    #expect(kept?.change == other && kept?.received == 10)
  }
}
