import AtlasOnboarding
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("OnboardingPresenter helper")
struct OnboardingPresenterHelperTests {
  /// The setup window's sample facts and snapshots, shared by the presenter's suites.
  typealias Samples = OnboardingSamples

  @Test("the panel counts its permission by this macOS's name, waits for the person, then says Allowed")
  func panelStatus() throws {
    var snapshot = Samples.snapshot(try Samples.helperSide(accessibility: true))
    snapshot.macOS = Samples.tahoe
    let off = OnboardingPresenter.panel(.screenRecording, snapshot: snapshot)
    #expect(off.label == "2 of 2 · Screen & System Audio Recording")
    #expect(off.instruction == "Drag Atlas into the top list.")
    #expect(off.status == .waiting("Waiting for you…") && !off.isOn, "the person acts, so no spinner")
    #expect(off.alreadyListed == "Already in the top list? Turn its switch on.")
    let on = OnboardingPresenter.panel(.accessibility, snapshot: snapshot)
    #expect(on.label == "1 of 2 · Accessibility")
    #expect(on.status == .allowed("Allowed") && on.isOn)
    snapshot.macOS = OperatingSystemVersion(majorVersion: 27, minorVersion: 0, patchVersion: 0)
    #expect(
      OnboardingPresenter.panel(.accessibility, snapshot: snapshot).label == "1 of 2 · Device Control and Data Access"
    )
  }

  @Test("while a permission just allowed shows its tick, the panel lights no other step, so its header and steps agree")
  func panelLightsOnlyTheOneItGuides() throws {
    var snapshot = Samples.snapshot(try Samples.helperSide(accessibility: true))
    snapshot.macOS = Samples.tahoe
    let ticking = OnboardingPresenter.panel(.accessibility, snapshot: snapshot)
    #expect(ticking.label == "1 of 2 · Accessibility")
    #expect(ticking.accessibility.isOn && !ticking.accessibility.isCurrent)
    #expect(!ticking.screenRecording.isCurrent, "Screen Recording lights up only once the panel moves on to it")
    let next = OnboardingPresenter.panel(.screenRecording, snapshot: snapshot)
    #expect(next.screenRecording.isCurrent && !next.accessibility.isCurrent)
  }

  @Test("Accessibility is allowed before Screen Recording, one at a time, from the footer")
  func accessibilityFirst() throws {
    var neither = Samples.snapshot(try Samples.helperSide())
    neither.macOS = Samples.tahoe
    let first = OnboardingPresenter.permissions(neither)
    #expect(first.accessibility.isCurrent && !first.screenRecording.isCurrent)
    #expect(first.accessibility.standing == .yourTurn, "the footer offers it, so its row says nothing")
    #expect(
      OnboardingPresenter.state(neither).footer.next
        == OnboardingFooterAction(
          title: .allowPermission("Accessibility"),
          event: .openSettings(id: Permission.accessibility.id),
          isEnabled: true,
          isDefault: true
        )
    )
    var one = Samples.snapshot(try Samples.helperSide(accessibility: true))
    one.macOS = Samples.tahoe
    let second = OnboardingPresenter.permissions(one)
    #expect(!second.accessibility.isCurrent && second.screenRecording.isCurrent)
    #expect(second.accessibility.standing == .allowed("Allowed"))
    #expect(second.screenRecording.title == "Screen & System Audio Recording")
    #expect(
      OnboardingPresenter.state(one).footer.next?.title == .allowPermission("Screen & System Audio Recording")
    )
    let both = OnboardingPresenter.permissions(
      Samples.snapshot(try Samples.helperSide(accessibility: true, screen: true))
    )
    #expect(!both.accessibility.isCurrent && !both.screenRecording.isCurrent)
  }

  @Test("a permission waiting its turn reads Next and dims its row, never Off")
  func aPermissionWaitingItsTurnReadsNext() throws {
    let screen = OnboardingPresenter.permissions(Samples.snapshot(try Samples.helperSide()))
    #expect(screen.screenRecording.standing == .next("Next"))
    #expect(screen.screenRecording.standing.dimsRow)
    #expect(!screen.accessibility.standing.dimsRow)
  }

  @Test("a permission turned off after this window saw it allowed reads Turned off, the one real problem")
  func aPermissionTurnedOffAgainSaysSo() throws {
    var snapshot = Samples.snapshot(try Samples.helperSide())
    #expect(OnboardingPresenter.permissions(snapshot).accessibility.standing == .yourTurn)
    snapshot.allowedBefore = [.accessibility]
    let screen = OnboardingPresenter.permissions(snapshot)
    #expect(screen.accessibility.standing == .turnedOff("Turned off"))
    #expect(screen.accessibility.isCurrent, "it is still the one to allow, from the footer")
    #expect(StatusWord.turnedOff.tone == .attention)
  }

  @Test("a permission whose state is unknown shows Checking…, never Off, and the footer offers nothing")
  func unknownPermissions() throws {
    var facts = try Samples.helperSide(accessibility: true)
    facts.asker.permissionsKnown = false
    let snapshot = Samples.snapshot(facts)
    let screen = OnboardingPresenter.permissions(snapshot)
    #expect(!screen.screenRecording.isCurrent, "nothing to allow while it is unknown")
    #expect(!screen.screenRecording.isKnown)
    #expect(screen.screenRecording.standing == .checking("Checking…"), "its row shows a spinner and Checking…")
    #expect(OnboardingPresenter.state(snapshot).footer.next == nil)
    let panel = OnboardingPresenter.panel(.screenRecording, snapshot: snapshot)
    #expect(panel.status == .checking("Checking…"), "the panel never says it waits for a switch it cannot read")
    #expect(!panel.isOn)
    #expect(!panel.screenRecording.isKnown && panel.screenRecording.standing == .checking("Checking…"))
    let read = OnboardingPresenter.panel(.screenRecording, snapshot: Samples.snapshot(try Samples.helperSide()))
    #expect(read.status == .waiting("Waiting for you…"), "a permission read as off waits for the person")
    #expect(screen.accessibility.standing == .allowed("Allowed"), "one known to be on stays Allowed")
  }

  @Test("the permissions screen's rows carry the title macOS uses on this version")
  func rowTitlesFollowMacOS() throws {
    var snapshot = Samples.snapshot(try Samples.helperSide())
    let titles = { (major: Int) in
      snapshot.macOS = OperatingSystemVersion(majorVersion: major, minorVersion: 0, patchVersion: 0)
      let screen = OnboardingPresenter.permissions(snapshot)
      return [screen.accessibility.title, screen.screenRecording.title]
    }
    #expect(titles(14) == ["Accessibility", "Screen Recording"])
    #expect(titles(15) == ["Accessibility", "Screen & System Audio Recording"])
    #expect(titles(26) == ["Accessibility", "Screen & System Audio Recording"])
    #expect(titles(27) == ["Device Control and Data Access", "Screen & System Audio Recording"])
  }

  @Test("the helper's done screen names the account to switch back to, or the person's own")
  func helperDoneTail() throws {
    let asked = OnboardingPresenter.helperDone(try Samples.helperSide(accessibility: true, screen: true).asker)
    #expect(asked.tail == "Switch back to Ismayil.")
    let unasked = OnboardingPresenter.helperDone(
      try Samples.helperSide(asker: "", accessibility: true, screen: true).asker
    )
    #expect(unasked.tail == "Switch back to your own account.")
  }
}
