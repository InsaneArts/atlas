import Foundation
import ServiceManagement
import Testing

@testable import AtlasCommands

@Suite("Login item")
struct LoginItemTests {
  @Test(
    "a change that already holds succeeds without a call, and one whose error leaves it holding succeeds after one",
    arguments: [
      (SMAppService.Status.notRegistered, SMAppService.Status.notRegistered, kSMErrorJobNotFound, false, [Bool]()),
      (.notFound, .notFound, kSMErrorJobNotFound, false, []),
      (.enabled, .enabled, kSMErrorAlreadyRegistered, true, []),
      (.notRegistered, .enabled, kSMErrorAlreadyRegistered, true, [true]),
      (.enabled, .notRegistered, kSMErrorJobNotFound, false, [false]),
      (.enabled, .notFound, kSMErrorJobNotFound, false, [false]),
    ]
  )
  func aChangeThatHoldsSucceeds(
    status: SMAppService.Status,
    after: SMAppService.Status,
    code: Int,
    on: Bool,
    calls: [Bool]
  ) throws {
    let service = ScriptedLoginItemService(
      status: status,
      statusAfterChange: after,
      failure: NSError(domain: "SMAppServiceErrorDomain", code: code)
    )

    try LoginItem(service: service)
      .set(on)

    #expect(service.calls == calls)
  }

  @Test(
    "a change fails with its error when the requested state does not hold, when the error is a real failure, or when it comes from another domain",
    arguments: [
      ("SMAppServiceErrorDomain", kSMErrorAlreadyRegistered, SMAppService.Status.requiresApproval, true),
      ("SMAppServiceErrorDomain", kSMErrorJobNotFound, .requiresApproval, false),
      ("SMAppServiceErrorDomain", kSMErrorInvalidSignature, .enabled, true),
      ("SMAppServiceErrorDomain", kSMErrorInvalidSignature, .notRegistered, false),
      (NSCocoaErrorDomain, kSMErrorAlreadyRegistered, .enabled, true),
      (NSCocoaErrorDomain, kSMErrorJobNotFound, .notRegistered, false),
    ]
  )
  func aChangeFailsWithItsError(domain: String, code: Int, after: SMAppService.Status, on: Bool) {
    let failure = NSError(domain: domain, code: code)
    let service = ScriptedLoginItemService(
      status: on ? .notRegistered : .enabled,
      statusAfterChange: after,
      failure: failure
    )

    #expect(throws: failure) {
      try LoginItem(service: service)
        .set(on)
    }
    #expect(service.calls == [on])
  }
}
