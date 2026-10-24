import AtlasOnboarding
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("OnboardingPresenter helper")
struct OnboardingPresenterHelperTests {
  /// The setup window's sample facts and snapshots, shared by the presenter's suites.
  typealias Samples = OnboardingSamples

  @Test("the panel counts its permission by this macOS's name and ticks its pill once the switch is on")
  func panelPill() throws {
    var snapshot = Samples.snapshot(try Samples.helperSide(accessibility: true))
    snapshot.macOS = Samples.tahoe
    let off = OnboardingPresenter.panel(.screenRecording, snapshot: snapshot)
    #expect(off.label == "2 of 2 · Screen & System Audio Recording")
    #expect(off.instruction == "Drag Atlas into the top list.")
    #expect(off.pill == "Waiting for the switch…" && !off.isOn)
    #expect(off.alreadyListed == "Already in the top list? Turn its switch on.")
    let on = OnboardingPresenter.panel(.accessibility, snapshot: snapshot)
    #expect(on.label == "1 of 2 · Accessibility")
    #expect(on.pill == "Allowed" && on.isOn)
    snapshot.macOS = OperatingSystemVersion(majorVersion: 27, minorVersion: 0, patchVersion: 0)
    #expect(
      OnboardingPresenter.panel(.accessibility, snapshot: snapshot).label == "1 of 2 · Device Control and Data Access"
    )
  }

  @Test("Accessibility is allowed before Screen Recording, one at a time")
  func accessibilityFirst() throws {
    var neither = Samples.snapshot(try Samples.helperSide())
    neither.macOS = Samples.tahoe
    let first = OnboardingPresenter.permissions(neither)
    #expect(first.accessibility.canAllow && !first.screenRecording.canAllow)
    #expect(first.accessibility.status == "Off" && first.accessibility.allowTitle == "Allow…")
    var one = Samples.snapshot(try Samples.helperSide(accessibility: true))
    one.macOS = Samples.tahoe
    let second = OnboardingPresenter.permissions(one)
    #expect(!second.accessibility.canAllow && second.screenRecording.canAllow)
    #expect(second.accessibility.status == "Allowed")
    #expect(second.screenRecording.title == "Screen & System Audio Recording")
    let both = OnboardingPresenter.permissions(
      Samples.snapshot(try Samples.helperSide(accessibility: true, screen: true))
    )
    #expect(!both.accessibility.canAllow && !both.screenRecording.canAllow)
  }

  @Test("a permission whose state is unknown shows Checking…, never Off")
  func unknownPermissions() throws {
    var facts = try Samples.helperSide(accessibility: true)
    facts.asker.permissionsKnown = false
    let snapshot = Samples.snapshot(facts)
    let screen = OnboardingPresenter.permissions(snapshot)
    #expect(!screen.screenRecording.canAllow, "nothing to allow while it is unknown")
    #expect(!screen.screenRecording.isKnown)
    #expect(screen.screenRecording.status == "Checking…")
    let standing = OnboardingPermissionStanding.of(screen.screenRecording, offersAllow: true)
    #expect(standing == .checking("Checking…"), "its row shows a spinner and Checking…")
    let panel = OnboardingPresenter.panel(.screenRecording, snapshot: snapshot)
    #expect(panel.pill == "Checking…", "the panel never says it waits for a switch it cannot read")
    #expect(!panel.isOn)
    #expect(!panel.screenRecording.isKnown && panel.screenRecording.status == "Checking…")
    let read = OnboardingPresenter.panel(.screenRecording, snapshot: Samples.snapshot(try Samples.helperSide()))
    #expect(read.pill == "Waiting for the switch…", "a permission read as off waits for its switch")
    #expect(
      OnboardingPermissionStanding.of(screen.accessibility, offersAllow: true) == .allowed("Allowed"),
      "one known to be on stays Allowed"
    )
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
