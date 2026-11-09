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

  /// What may take the Screen from the person mid-sign-in, and how it comes back.
  enum Cause: String, CaseIterable, Sendable {
    /// Another app takes the keyboard, as a password manager does.
    case focus
    /// The window is covered or hidden.
    case covered
    /// The picture is paused.
    case paused
    /// The window shows Activity.
    case elsewhere
  }

  @Test(
    "each way the Screen goes only stops input, keeps the lease, and input goes again once it is back",
    arguments: Cause.allCases
  )
  func eachCauseOnlySuspends(cause: Cause) async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    _ = await viewer.startSignIn(.signIn, site: "github.com", browser: nil)
    switch cause {
    case .focus: viewer.control.lostFocus()
    case .covered: viewer.setVisible(false)
    case .paused: viewer.togglePause()
    case .elsewhere: viewer.show(.activity)
    }
    viewer.control.sendKey("a")
    #expect(viewer.control.signInLease == "sign-in-lease" && viewer.holdingControl)
    #expect(!service.log.contains { $0.hasPrefix("release") || $0.hasPrefix("cancel") }, "nothing is given back")
    switch cause {
    case .focus: viewer.control.gainedFocus()
    case .covered: viewer.setVisible(true)
    case .paused: viewer.togglePause()
    case .elsewhere: viewer.show(.screen)
    }
    try await settle("the picture never came back") { viewer.reading.frame != nil && !viewer.control.pictureStale }
    viewer.control.sendKey("b")
    try await settle("input never went again") { service.actions.contains([.key("b")]) }
    #expect(!service.actions.contains([.key("a")]), "nothing went while the Screen was away")
  }

  @Test("covered past a lease period, the sign-in's lease is renewed with no picture and kept")
  func theLeaseOutlivesItsPeriod() async throws {
    let service = ScriptedService()
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    _ = await viewer.startSignIn(.signIn, site: "github.com", browser: nil)
    viewer.setVisible(false)
    // Eleven seconds, past the ten a lease lasts unrenewed.
    for _ in 0..<22 {
      try await clock.untilSleeping(for: ViewerControl.heartbeatEvery, "the sign-in's heartbeat never waited")
      clock.advance(by: ViewerControl.heartbeatEvery)
    }
    try await settle("the lease was not renewed through the period") {
      service.log.filter { $0 == "renew sign-in sign-in-lease" }.count >= 4
    }
    #expect(viewer.control.signInLease == "sign-in-lease")
    #expect(!service.log.contains { $0.hasPrefix("release") || $0.hasPrefix("cancel") })
  }

  @Test("ending control on purpose, as quitting and closing the window do, cancels the sign-in at once and never hangs")
  func endingOnPurposeCancels() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    _ = await viewer.startSignIn(.signIn, site: "github.com", browser: nil)
    // What ⌘Q and every helper command do first: release until the window holds nothing.
    viewer.control.releaseControl()
    #expect(!viewer.holdingControl && !viewer.control.acquiring, "the quit loop ends")
    _ = await viewer.actions.barrier.settle()
    #expect(service.log.contains("cancel sign-in-lease"))
    #expect(viewer.state.screen.signingIn == nil)
    _ = await viewer.startSignIn(.signIn, site: "github.com", browser: nil)
    viewer.close()
    _ = await viewer.actions.barrier.settle()
    #expect(service.log.filter { $0 == "cancel sign-in-lease" }.count == 2, "closing the window cancels it too")
  }

  @Test("Cancel ends the hold and adds nothing, and Give Back during a sign-in is its Cancel")
  func cancelEndsIt() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    _ = await viewer.startSignIn(.signIn, site: "github.com", browser: nil)
    viewer.send(.giveBack)
    try await settle("Give Back never cancelled") { viewer.control.signInLease == nil }
    #expect(await viewer.actions.barrier.settle() == nil, "the Cancel was answered")
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
    for kind in [SignInPurpose.forget, .reset] {
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

  @Test(
    "Sign Out holds the helper on the site's own page, and Keep's words follow the account's level",
    arguments: [
      ("lookOnly", "Agents can only look there while it stays listed."),
      ("ask", "Agents ask you before sending there while it stays listed."),
      ("trusted", "Agents act there without asking while it stays listed."),
    ]
  )
  func signOutAndKeepWords(level: String, words: String) async throws {
    let service = ScriptedService()
    service.reply = [
      "ready": true, "canObserve": true,
      "overview": [
        "mode": "observing",
        "accounts": [["kind": "site", "id": "github.com", "name": "github.com", "added": 1, "level": level]],
      ],
    ]
    let viewer = try await ViewerController.watching(service)
    _ = await viewer.startSignIn(.signOut, site: "github.com", browser: nil)
    #expect(service.log.contains("start signIn github.com"), "signing out opens the site as signing in does")
    #expect(viewer.state.screen.signingIn?.text.hasPrefix("Sign out of github.com on its page") == true)
    await viewer.finishSignIn()
    #expect(viewer.state.alert?.title == "Remove github.com from Accounts too?")
    #expect(viewer.state.alert?.message == words)
  }

  @Test("a held flow started from a paused picture resumes it, and input goes only once a fresh picture arrives")
  func aPausedPictureResumes() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    viewer.togglePause()
    _ = await viewer.startSignIn(.signIn, site: "github.com", browser: nil)
    #expect(!viewer.session.paused)
    viewer.control.sendKey("a")
    #expect(!service.actions.contains([.key("a")]), "nothing goes before a fresh picture")
    try await settle("no fresh picture came") { viewer.reading.frame != nil && !viewer.control.pictureStale }
    viewer.control.sendKey("b")
    try await settle("input never went") { service.actions.contains([.key("b")]) }
  }

  @Test("input goes again after a lost picture comes back and after an alert is dismissed")
  func inputComesBackAfterRecoveryAndAlerts() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    _ = await viewer.startSignIn(.signIn, site: "github.com", browser: nil)
    viewer.readingLoop.forgetFrame()
    viewer.readingLoop.refresh()
    try await settle("the picture never came back") { viewer.reading.frame != nil }
    viewer.control.sendKey("a")
    try await settle("input never went after the picture came back") { service.actions.contains([.key("a")]) }
    viewer.ask(.restart)
    viewer.cancelAlert()
    viewer.control.sendKey("b")
    try await settle("input never went after the alert") { service.actions.contains([.key("b")]) }
    #expect(viewer.control.signInLease == "sign-in-lease")
  }

  @Test("the result speaks for the entry of the browser signed in to, never another browser's for the same site")
  func theResultMatchesItsBrowser() async throws {
    let service = ScriptedService()
    service.reply = [
      "ready": true, "canObserve": true,
      "overview": [
        "mode": "observing",
        "accounts": [
          [
            "kind": "site", "id": "github.com", "name": "github.com", "browser": "org.mozilla.firefox", "added": 1,
            "level": "ask",
          ],
          [
            "kind": "site", "id": "github.com", "name": "github.com", "browser": "com.google.Chrome", "added": 1,
            "level": "trusted",
          ],
        ],
      ],
    ]
    let viewer = try await ViewerController.watching(service)
    _ = await viewer.startSignIn(.signIn, site: "github.com", browser: "com.google.Chrome")
    await viewer.finishSignIn()
    #expect(viewer.state.alert?.message == "Agents act there without asking.")
    viewer.cancelAlert()
    _ = await viewer.startSignIn(.signIn, site: "github.com", browser: "org.mozilla.firefox")
    await viewer.finishSignIn()
    #expect(viewer.state.alert?.message == "Agents ask you before sending there.")
  }

  @Test("closing the window during a slow start calls it off, and the late grant is cancelled, never held")
  func closingDuringAStartCallsItOff() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    service.hold.insert("sign-in/start")
    let starting = Task { await viewer.startSignIn(.signIn, site: "github.com", browser: nil) }
    try await settle("the start never reached the service") { service.held["sign-in/start"] != nil }
    #expect(viewer.control.acquiring, "the start counts as taking control")
    viewer.close()
    #expect(!viewer.holdingControl && !viewer.control.acquiring, "closing ends the wait at once")
    service.resume("sign-in/start")
    #expect(await starting.value == SignInWords.calledOff)
    _ = await viewer.actions.barrier.settle()
    #expect(service.log.contains("cancel sign-in-lease"), "the late grant is cancelled")
    #expect(viewer.control.signInLease == nil && viewer.session.signIn == nil)
  }

  @Test("closing the window while control held before is given back calls the start off before it is sent")
  func closingDuringThePreliminaryReleaseSendsNoStart() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.control.takeControl()
    try #require(viewer.holdingControl)
    service.hold.insert("release")
    let starting = Task { await viewer.startSignIn(.signIn, site: "github.com", browser: nil) }
    try await settle("the release never reached the service") { service.held["release"] != nil }
    viewer.close()
    service.resume("release")
    #expect(await starting.value == SignInWords.calledOff)
    #expect(!service.log.contains { $0.hasPrefix("start") }, "no start was sent")
    #expect(viewer.control.signInLease == nil && !viewer.control.acquiring)
  }

  @Test("suspending with a button down sends its release first, keeps the lease, and the next click works")
  func aHeldButtonComesUpOnSuspend() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    let size = CGSize(width: 400, height: 300)
    let middle = CGPoint(x: 200, y: 150)
    _ = await viewer.startSignIn(.signIn, site: "github.com", browser: nil)
    viewer.control.pointerDown(.left, at: middle, in: size, modifiers: [])
    try await settle("the press was never sent") { service.actions.count == 1 }
    viewer.control.lostFocus()
    try await settle("the release was never sent") { service.actions.count == 2 }
    #expect(
      service.actions.last?.first.map { if case .pointer(.up, _, _, _, _, _) = $0 { true } else { false } } == true
    )
    #expect(viewer.control.signInLease == "sign-in-lease")
    viewer.control.gainedFocus()
    viewer.control.pointerDown(.left, at: middle, in: size, modifiers: [])
    viewer.control.pointerUp(.left, at: middle, in: size, modifiers: [])
    try await settle("the next click never went") { service.actions.count == 4 }
    #expect(!service.log.contains { $0.hasPrefix("cancel") || $0.hasPrefix("release") })
  }

  @Test("a Cancel that fails stays to be sent again, and is sent before control is taken again")
  func aFailedCancelIsSentAgain() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    _ = await viewer.startSignIn(.signIn, site: "github.com", browser: nil)
    service.fail.insert("sign-in/cancel")
    viewer.control.releaseControl()
    #expect(await viewer.actions.barrier.settle() == "sign-in/cancel failed", "the helper isn't free yet")
    #expect(viewer.actions.barrier.uncancelled == ["sign-in-lease"])
    service.fail.remove("sign-in/cancel")
    await viewer.control.takeControl()
    #expect(viewer.actions.barrier.uncancelled.isEmpty)
    let cancels = service.log.indices.filter { service.log[$0] == "cancel sign-in-lease" }
    let acquire = try #require(service.log.lastIndex { $0.hasPrefix("acquire") })
    #expect(cancels.count >= 2, "the failed Cancel is sent again")
    #expect(cancels.last.map { $0 < acquire } == true, "before control is taken")
  }

  @Test("a Done that answers after its sign-in ended and another began leaves the newer one alone")
  func aLateDoneLeavesTheNewerSignIn() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    _ = await viewer.startSignIn(.signIn, site: "github.com", browser: nil)
    service.hold.insert("sign-in/done")
    let finishing = Task { await viewer.finishSignIn() }
    try await settle("Done never reached the service") { service.held["sign-in/done"] != nil }
    // The release shortcut ends it on purpose, and the person starts again: the start waits for Done's answer.
    viewer.control.releaseControl()
    service.signInLease = "second-lease"
    let starting = Task { await viewer.startSignIn(.signIn, site: "gitlab.com", browser: nil) }
    service.resume("sign-in/done")
    await finishing.value
    #expect(await starting.value == nil)
    #expect(viewer.control.signInLease == "second-lease", "the newer lease is kept")
    #expect(viewer.session.signIn?.site == "gitlab.com")
    #expect(viewer.state.alert == nil, "the late Done says nothing")
    #expect(viewer.state.screen.signingIn?.isWorking == false, "the newer bar's buttons take presses")
  }

  @Test("once Done is sent no renewal runs, and one failing under it leaves Done's result whole")
  func doneOwnsTheLease() async throws {
    let service = ScriptedService()
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    _ = await viewer.startSignIn(.forget, site: "github.com", browser: nil)
    // Covered, so only the sign-in's heartbeat waits on the clock.
    viewer.setVisible(false)
    service.hold.insert("sign-in/renew")
    service.refusals["sign-in/renew"] = ServiceError("The lease ended.", code: ControlBarrier.leaseLost)
    for _ in 0..<ViewerControl.renewEvery {
      try await clock.untilSleeping(for: ViewerControl.heartbeatEvery, "the heartbeat never waited")
      clock.advance(by: ViewerControl.heartbeatEvery)
    }
    try await settle("no renewal was under way") { service.held["sign-in/renew"] != nil }
    service.hold.insert("sign-in/done")
    let finishing = Task { await viewer.finishSignIn() }
    try await settle("Done never reached the service") { service.held["sign-in/done"] != nil }
    service.resume("sign-in/renew")
    clock.advance(by: ViewerControl.heartbeatEvery * ViewerControl.renewEvery)
    service.resume("sign-in/done")
    await finishing.value
    #expect(viewer.state.alert?.title == "Remove github.com from Accounts too?", "Done's result stands")
    #expect(service.log.filter { $0.hasPrefix("renew sign-in") }.count == 1, "no renewal after Done was sent")
  }

  @Test("a start whose reply is lost holds the helper's commands until a hold it may have begun can't be alive")
  func aLostStartIsHeld() async throws {
    let service = ScriptedService()
    service.fail.insert("sign-in/start")
    let viewer = try await ViewerController.watching(service)
    #expect(await viewer.startSignIn(.signIn, site: "github.com", browser: nil) == "sign-in/start failed")
    #expect(viewer.actions.barrier.isBusy, "the unknown outcome holds what needs the helper free")
    #expect(viewer.control.signInLease == nil)
  }

  @Test("every start first sends again a Cancel still unanswered")
  func aStartSendsAPendingCancelFirst() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    _ = await viewer.startSignIn(.signIn, site: "github.com", browser: nil)
    service.fail.insert("sign-in/cancel")
    await viewer.cancelSignIn()
    #expect(viewer.actions.barrier.uncancelled == ["sign-in-lease"])
    service.fail.remove("sign-in/cancel")
    #expect(await viewer.startSignIn(.signIn, site: "gitlab.com", browser: nil) == nil)
    let lastCancel = try #require(service.log.lastIndex(of: "cancel sign-in-lease"))
    let secondStart = try #require(service.log.lastIndex(of: "start signIn gitlab.com"))
    #expect(lastCancel < secondStart, "the Cancel went first")
    #expect(viewer.actions.barrier.uncancelled.isEmpty)
  }

  @Test(
    "a renewal that fails without saying the lease is lost cancels the sign-in, a button let up first; a lost lease only ends it here",
    arguments: [false, true]
  )
  func aFailedRenewalCancelsUnlessTheLeaseIsLost(leaseLost: Bool) async throws {
    let service = ScriptedService()
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    _ = await viewer.startSignIn(.signIn, site: "github.com", browser: nil)
    viewer.control.pointerDown(.left, at: CGPoint(x: 200, y: 150), in: CGSize(width: 400, height: 300), modifiers: [])
    try await settle("the press was never sent") { service.actions.count == 1 }
    // Reading stops, so only the sign-in's heartbeat waits on the clock.
    viewer.readingLoop.watch(false)
    if leaseLost {
      service.refusals["sign-in/renew"] = ServiceError("The lease ended.", code: ControlBarrier.leaseLost)
    } else {
      service.fail.insert("sign-in/renew")
    }
    for _ in 0..<ViewerControl.renewEvery {
      try await clock.untilSleeping(for: ViewerControl.heartbeatEvery, "the heartbeat never waited")
      clock.advance(by: ViewerControl.heartbeatEvery)
    }
    try await settle("the sign-in never ended") { viewer.control.signInLease == nil }
    _ = await viewer.actions.barrier.settle()
    #expect(service.log.contains("cancel sign-in-lease") == !leaseLost)
    if !leaseLost {
      let upIndex = try #require(service.log.lastIndex { $0.hasPrefix("input") })
      let cancelIndex = try #require(service.log.lastIndex(of: "cancel sign-in-lease"))
      #expect(service.actions.count == 2 && upIndex < cancelIndex, "the button went up before the Cancel")
    }
  }

  @Test("a start the service refuses says why and holds nothing")
  func aRefusalIsSaid() async throws {
    let service = ScriptedService()
    service.refusals["sign-in/start"] = ServiceError("An agent's command is still running.", code: "busy")
    let viewer = try await ViewerController.watching(service)
    let refusal = await viewer.startSignIn(.signIn, site: "github.com", browser: nil)
    #expect(refusal == "An agent's command is still running.")
    #expect(!viewer.actions.barrier.isBusy, "the service's own refusal says no hold began")
    #expect(viewer.control.signInLease == nil && viewer.state.screen.signingIn == nil)
  }
}
