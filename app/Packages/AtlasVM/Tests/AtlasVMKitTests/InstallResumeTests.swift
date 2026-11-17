import Foundation
import Testing

@testable import AtlasVMKit

@Suite("An install gone on with without its secret")
struct InstallResumeTests {
  let install = ChangeID.repeating("1a")
  let request = InstallRequest(bundle: "/Applications/Atlas.app", enrolment: "e", secret: "the secret")

  func service(_ host: FakeHost, in folders: Folders) throws -> (ControlService, ChangeLedger) {
    let ledger = ChangeLedger(folder: try folders.folder("changes"))
    let incarnation = try #require(Incarnation.current())
    return (ControlService(ledger: ledger, host: host, incarnation: incarnation), ledger)
  }

  @Test("an install left pending goes on when asked without its secret, as the install it was, and no change is kept")
  func goesOn() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let (service, ledger) = try service(host, in: folders)
    _ = await service.answer(.install(install, request))

    let reply = await service.answer(.installResume(install))

    #expect(reply.ok && reply.pending == true && reply.change == install)
    #expect(await host.resumed == [install])
    #expect(await host.installs == [request], "the secret was not asked for again")
    #expect(try ledger.unsettled().map(\.operation) == [.install(install, request.recorded)])
  }

  @Test("going on with a change that is no install is refused, and the VM's host never hears it")
  func noInstall() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .stopped)
    let (service, ledger) = try service(host, in: folders)
    let start = ChangeID.repeating("2b")
    _ = await service.answer(.start(start))

    let unknown = await service.answer(.installResume(ChangeID.repeating("3c")))
    let other = await service.answer(.installResume(start))

    #expect(!unknown.ok && !other.ok)
    #expect(await host.resumed.isEmpty)
    #expect(try ledger.record(ChangeID.repeating("3c")) == nil)
  }

  @Test("an install the station's word closed answers that it is done, and does not go on")
  func settled() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let (service, _) = try service(host, in: folders)
    _ = await service.answer(.install(install, request))
    _ = await service.answer(.closed(ChangeID.repeating("4d"), install: install))

    let reply = await service.answer(.installResume(install))

    #expect(reply.outcome == .done && reply.pending == false)
    #expect(await host.resumed.isEmpty)
  }

  @Test("a refusal for good while going on settles the install it goes on with")
  func refusedForGood() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let (service, ledger) = try service(host, in: folders)
    _ = await service.answer(.install(install, request))
    await host.resuming(as: .refused("The staged Atlas is not signed as Atlas."))

    let reply = await service.answer(.installResume(install))
    let again = await service.answer(.installResume(install))

    #expect(reply.outcome == .refused("The staged Atlas is not signed as Atlas."))
    #expect(try ledger.record(install)?.outcome == .refused("The staged Atlas is not signed as Atlas."))
    #expect(again.outcome == reply.outcome)
    #expect(await host.resumed == [install], "a settled install does not go on again")
  }

  @Test("going on with an install is read from one request line with its change alone, and the runner declares it")
  func line() throws {
    let line = Data(#"{"change":"\#(install.text)","op":"install-resume"}"#.utf8)

    let operation = try JSONDecoder()
      .decode(ControlOperation.self, from: line)
    let encoder = JSONEncoder()
    encoder.outputFormatting = .sortedKeys

    #expect(operation == .installResume(install))
    #expect(operation.change == install && operation.name == "install-resume")
    #expect(try encoder.encode(operation) == line)
    #expect(ControlHello.ours.capabilities.contains("install-resume"))
  }
}
