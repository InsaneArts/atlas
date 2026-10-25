import Foundation
import ServiceManagement
import Testing

@testable import AtlasCommands

@Suite("Login item")
struct LoginItemTests {
  @Test(
    "turning off an unregistered login item succeeds without unregistering",
    arguments: [SMAppService.Status.notRegistered, .notFound]
  )
  func turningOffAnUnregisteredLoginItem(status: SMAppService.Status) throws {
    let service = ScriptedLoginItemService(
      status: status,
      statusAfterChange: status,
      failure: NSError(domain: "SMAppServiceErrorDomain", code: kSMErrorJobNotFound)
    )

    try LoginItem(service: service)
      .set(false)

    #expect(service.calls.isEmpty)
  }

  @Test("turning on an enabled login item succeeds without registering")
  func turningOnAnEnabledLoginItem() throws {
    let service = ScriptedLoginItemService(
      status: .enabled,
      statusAfterChange: .enabled,
      failure: NSError(domain: "SMAppServiceErrorDomain", code: kSMErrorAlreadyRegistered)
    )

    try LoginItem(service: service)
      .set(true)

    #expect(service.calls.isEmpty)
  }

  @Test("an already registered error succeeds when the service is now enabled")
  func anAlreadyRegisteredErrorReconciles() throws {
    let service = ScriptedLoginItemService(
      status: .notRegistered,
      statusAfterChange: .enabled,
      failure: NSError(domain: "SMAppServiceErrorDomain", code: kSMErrorAlreadyRegistered)
    )

    try LoginItem(service: service)
      .set(true)

    #expect(service.calls == [true])
  }

  @Test(
    "a job not found error succeeds when the service is now unregistered",
    arguments: [SMAppService.Status.notRegistered, .notFound]
  )
  func aJobNotFoundErrorReconciles(status: SMAppService.Status) throws {
    let service = ScriptedLoginItemService(
      status: .enabled,
      statusAfterChange: status,
      failure: NSError(domain: "SMAppServiceErrorDomain", code: kSMErrorJobNotFound)
    )

    try LoginItem(service: service)
      .set(false)

    #expect(service.calls == [false])
  }

  @Test("a registration error still fails when the requested state does not hold", arguments: [true, false])
  func anUnreconciledErrorFails(on: Bool) {
    let failure = NSError(
      domain: "SMAppServiceErrorDomain",
      code: on ? kSMErrorAlreadyRegistered : kSMErrorJobNotFound
    )
    let service = ScriptedLoginItemService(
      status: on ? .notRegistered : .enabled,
      statusAfterChange: .requiresApproval,
      failure: failure
    )

    #expect(throws: failure) {
      try LoginItem(service: service)
        .set(on)
    }
    #expect(service.calls == [on])
  }

  @Test("a real failure is preserved even when the requested state now holds", arguments: [true, false])
  func aRealFailureIsPreserved(on: Bool) {
    let failure = NSError(domain: "SMAppServiceErrorDomain", code: kSMErrorInvalidSignature)
    let service = ScriptedLoginItemService(
      status: on ? .notRegistered : .enabled,
      statusAfterChange: on ? .enabled : .notRegistered,
      failure: failure
    )

    #expect(throws: failure) {
      try LoginItem(service: service)
        .set(on)
    }
    #expect(service.calls == [on])
  }

  @Test("an error from another domain is preserved even when its code matches", arguments: [true, false])
  func anUnrelatedErrorIsPreserved(on: Bool) {
    let failure = NSError(
      domain: NSCocoaErrorDomain,
      code: on ? kSMErrorAlreadyRegistered : kSMErrorJobNotFound
    )
    let service = ScriptedLoginItemService(
      status: on ? .notRegistered : .enabled,
      statusAfterChange: on ? .enabled : .notRegistered,
      failure: failure
    )

    #expect(throws: failure) {
      try LoginItem(service: service)
        .set(on)
    }
    #expect(service.calls == [on])
  }
}
