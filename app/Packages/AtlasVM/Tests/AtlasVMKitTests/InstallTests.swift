import Foundation
import Testing

@testable import AtlasVMKit

@Suite("An install asked of the control service")
struct InstallTests {
  let install = ChangeID.repeating("1a")
  let request = InstallRequest(bundle: "/Applications/Atlas.app", enrolment: "e", secret: "the secret")

  @Test("an install's secret is never kept, and asked again with it the same install goes on")
  func secretNeverKept() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let ledger = ChangeLedger(folder: try folders.folder("changes"))
    let service = ControlService(ledger: ledger, host: host, incarnation: try #require(Incarnation.current()))

    let first = await service.answer(.install(install, request))
    let again = await service.answer(.install(install, request))

    #expect(first.pending == true && again.pending == true)
    let kept = try Data(contentsOf: ledger.folder.appending(path: "\(install.text).json"))
    #expect(!String(decoding: kept, as: UTF8.self).contains("the secret"))
    #expect(await host.installs == [request, request])
    #expect(try ledger.unsettled().count == 1)
  }

  @Test("the station's reports are served while an install waits, and closed settles the install")
  func reports() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let ledger = ChangeLedger(folder: try folders.folder("changes"))
    let service = ControlService(ledger: ledger, host: host, incarnation: try #require(Incarnation.current()))
    _ = await service.answer(.install(install, request))

    let paired = await service.answer(
      .paired(ChangeID.repeating("2b"), install: install, enrolment: "e")
    )
    let closed = await service.answer(.closed(ChangeID.repeating("3c"), install: install))

    #expect(paired.outcome == .done && closed.outcome == .done)
    #expect(try ledger.record(install)?.outcome == .done)
    #expect(await host.reports == ["paired", "closed"])
  }

  @Test("a paired report for another enrolment than its install's is refused before the host hears it")
  func otherEnrolment() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let ledger = ChangeLedger(folder: try folders.folder("changes"))
    let service = ControlService(ledger: ledger, host: host, incarnation: try #require(Incarnation.current()))
    _ = await service.answer(.install(install, request))

    let paired = await service.answer(
      .paired(ChangeID.repeating("2b"), install: install, enrolment: "another")
    )

    #expect(paired.outcome == .refused("That is not the enrolment of that install."))
    #expect(await host.reports.isEmpty)
  }

  @Test("a report that could not be kept goes on when asked again, and closed settles its install first")
  func reportAskedAgain() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let ledger = ChangeLedger(folder: try folders.folder("changes"))
    let service = ControlService(ledger: ledger, host: host, incarnation: try #require(Incarnation.current()))
    _ = await service.answer(.install(install, request))
    let report = ChangeID.repeating("3c")
    await host.sticking(reports: true)

    let first = await service.answer(.closed(report, install: install))
    await host.sticking(reports: false)
    let again = await service.answer(.closed(report, install: install))

    #expect(first.pending == true && again.outcome == .done)
    #expect(try ledger.record(install)?.outcome == .done)
    #expect(await host.reports == ["closed", "closed"])
  }
}
