import Foundation
import Testing
import os

@testable import AtlasVMKit

@Suite("The newest macOS this Mac's framework supports")
struct LatestMacOSTests {
  @Test("latest is asked of the framework at most once a day")
  func onceADay() async throws {
    let folders = try Folders()
    let file = try folders.folder("images")
      .appending(path: "Latest.json")
    let clock = ManualClock()
    let asked = OSAllocatedUnfairLock(initialState: 0)
    let ask: @Sendable () async throws -> MacOSRelease = {
      asked.withLock { $0 += 1 }
      return MacOSRelease(major: 27, minor: 2, patch: 0, build: "27C101")
    }
    let latest = LatestMacOS(file: file, now: { clock.today }, ask: ask)

    let first = await latest.release()
    clock.advance(by: .seconds(23 * 60 * 60))
    let later = await latest.release()
    // Another run, reading what was kept.
    let anotherRun = LatestMacOS(file: file, now: { clock.today }, ask: ask)
    let another = await anotherRun.release()
    let withinADay = asked.withLock { $0 }
    clock.advance(by: .seconds(2 * 60 * 60))
    let nextDay = await latest.release()

    #expect(first == MacOSRelease(version: "27.2", build: "27C101"))
    #expect(later == first && another == first)
    #expect(withinADay == 1)
    #expect(nextDay == first && asked.withLock { $0 } == 2)
  }

  @Test("an asking that failed says none and is tried again after an hour")
  func failed() async throws {
    let folders = try Folders()
    let file = try folders.folder("images")
      .appending(path: "Latest.json")
    let clock = ManualClock()
    let asked = OSAllocatedUnfairLock(initialState: 0)
    let ask: @Sendable () async throws -> MacOSRelease = {
      let count = asked.withLock { asked in
        asked += 1
        return asked
      }
      guard count > 1 else { throw CocoaError(.featureUnsupported) }
      return MacOSRelease(version: "27.3", build: "27D50")
    }
    let latest = LatestMacOS(file: file, now: { clock.today }, ask: ask)

    let first = await latest.release()
    let soon = await latest.release()
    clock.advance(by: .seconds(61 * 60))
    let hourLater = await latest.release()

    #expect(first == nil && soon == nil)
    #expect(hourLater?.version == "27.3")
    #expect(asked.withLock { $0 } == 2)
  }
}
