import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerController, a held sign-in")
@MainActor
struct ViewerSignInTests {
  @Test("Sign In starts, holds the lease as control and shows the bar and the tag, and Done adds the site and says so")
  func theFlowRunsInOrder() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    let refusal = await viewer.startSignIn(.signIn, site: "github.com", browser: nil)
    #expect(refusal == nil)
    #expect(viewer.control.signInLease == "sign-in-lease" && viewer.control.lease == "sign-in-lease")
    #expect(
      viewer.state.screen.signingIn?.text == "Signing in to github.com. Agents are paused and can't see the screen."
    )
    #expect(viewer.state.header.tag == "Screen · Hidden from agents")
    await viewer.finishSignIn()
    #expect(
      service.log.filter { $0.hasPrefix("start") || $0.hasPrefix("done") } == [
        "start signIn github.com", "done sign-in-lease",
      ]
    )
    #expect(viewer.control.lease == nil && viewer.control.signInLease == nil)
    #expect(viewer.state.screen.signingIn == nil)
    #expect(viewer.state.alert?.title == "Added github.com to Accounts")
    #expect(viewer.state.alert?.message == "Agents ask you before sending there.")
    #expect(
      !service.log.contains { $0.hasPrefix("release") },
      "Done ends the lease at the service; nothing else is given back"
    )
  }

  @Test(
    "losing focus, being covered or leaving the Screen only stops input: the lease is kept and renewed with no picture"
  )
  func theHoldSurvivesEveryRelease() async throws {
    let service = ScriptedService()
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    _ = await viewer.startSignIn(.signIn, site: "github.com", browser: nil)
    viewer.control.lostFocus()
    viewer.setVisible(false)
    viewer.show(.activity)
    viewer.send(.menu(.togglePause))
    #expect(viewer.control.signInLease == "sign-in-lease" && viewer.control.lease == "sign-in-lease")
    #expect(!viewer.control.canInput, "input stops while the window doesn't take it")
    #expect(!service.log.contains { $0.hasPrefix("release") }, "nothing is given back")
    for _ in 0..<ViewerControl.renewEvery {
      try await clock.untilSleeping(for: ViewerControl.heartbeatEvery, "the sign-in's heartbeat never waited")
      clock.advance(by: ViewerControl.heartbeatEvery)
    }
    try await settle("the lease was never renewed") { service.log.contains("renew sign-in sign-in-lease") }
    viewer.control.gainedFocus()
    #expect(viewer.control.lease == "sign-in-lease")
  }

  @Test("Cancel ends the hold and adds nothing, and Give Back during a sign-in is its Cancel")
  func cancelEndsIt() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    _ = await viewer.startSignIn(.signIn, site: "github.com", browser: nil)
    viewer.send(.giveBack)
    try await settle("Give Back never cancelled") { viewer.control.signInLease == nil }
    #expect(service.log.contains("cancel sign-in-lease"))
    #expect(viewer.state.screen.signingIn == nil && viewer.state.alert == nil)
  }

  @Test("after Forget or Reset, Done asks whether to remove the site too, Keep the default, and only Remove removes it")
  func forgetAsksToRemoveKeepFirst() async throws {
    let service = ScriptedService()
    service.reply = [
      "ready": true, "canObserve": true,
      "overview": [
        "mode": "observing",
        "accounts": [
          ["kind": "site", "id": "github.com", "name": "github.com", "browser": "com.google.Chrome", "added": 1]
        ],
      ],
    ]
    let viewer = try await ViewerController.watching(service)
    for kind in [SignInKind.forget, .reset] {
      _ = await viewer.startSignIn(kind, site: "github.com", browser: "com.google.Chrome")
      #expect(viewer.state.screen.signingIn?.text.hasPrefix(kind == .forget ? "Delete github.com" : "Clear") == true)
      await viewer.finishSignIn()
      let alert = try #require(viewer.state.alert)
      #expect(alert.title == "Remove github.com from Accounts too?")
      #expect(alert.buttons.map(\.title) == ["Keep", "Remove"])
      #expect(alert.buttons.first?.role == .safeCancel, "Keep is the default")
      viewer.cancelAlert()
      #expect(!service.log.contains("remove account github.com"), "Keep removes nothing")
    }
    _ = await viewer.startSignIn(.forget, site: "github.com", browser: "com.google.Chrome")
    await viewer.finishSignIn()
    await viewer.confirmAlert()
    #expect(service.log.contains("remove account github.com"))
  }

  @Test("a start the service refuses says why and holds nothing")
  func aRefusalIsSaid() async throws {
    let service = ScriptedService()
    service.fail.insert("sign-in/start")
    let viewer = try await ViewerController.watching(service)
    let refusal = await viewer.startSignIn(.signIn, site: "github.com", browser: nil)
    #expect(refusal == "sign-in/start failed")
    #expect(viewer.control.signInLease == nil && viewer.state.screen.signingIn == nil)
  }
}
