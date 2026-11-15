import Foundation
import Testing

@testable import AtlasVMKit

@Suite("The runner's lock")
struct RunnerLockTests {
  @Test("one run holds a VM's lock at a time, and another takes it once the first lets go")
  func oneAtATime() throws {
    let folders = try Folders()
    let file = folders.root.appending(path: "Runner.lock")
    var first = RunnerLock(at: file)
    #expect(first != nil)

    #expect(RunnerLock(at: file) == nil)
    first = nil
    #expect(RunnerLock(at: file) != nil)
  }
}
