import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("Viewer desktops")
@MainActor
struct ViewerControllerDesktopsTests {
  @Test("with one desktop nothing changes in the window: the same foot row, header and click")
  func withOneDesktopNothingChanges() throws {
    for (mode, state) in [("agent", "working"), ("controlling", "control"), ("unavailable", "notReady")] {
      var snapshot = try ViewerPresenterHelperLineTests.reading(mode, state)
      let before = ViewerPresenter.state(snapshot)
      snapshot.desktops = DesktopLists.one
      #expect(ViewerPresenter.state(snapshot) == before, "\(mode)")
      #expect(!ViewerPresenter.helperLine(snapshot).picksDesktop)
    }
  }

  @Test("a VM shown is named in the foot row with its size and in the header, and a click opens the desktops")
  func aVMShownIsNamed() throws {
    var snapshot = try ViewerPresenterHelperLineTests.reading("observing", "idle")
    snapshot.desktops = DesktopLists.two(shown: "work", work: DesktopLists.work(.idle))
    let line = ViewerPresenter.helperLine(snapshot)
    #expect(line.title == "Atlas is ready")
    #expect(line.line == "VM work · 1440 × 900", "a VM is never the helper")
    #expect(line.desktops.map(\.title) == ["Second Login", "work"])
    #expect(line.desktops.map(\.isViewed) == [false, true])
    snapshot.lease = "lease"
    #expect(ViewerPresenter.header(snapshot).what == "have control of work")
    snapshot.desktops = DesktopLists.two(shown: "second-login")
    #expect(ViewerPresenter.helperLine(snapshot).line == "Helper atlas · 1440 × 900", "Second Login keeps its line")
    #expect(ViewerPresenter.holderWords(snapshot) == "has Second Login")
    snapshot.desktops = nil
    #expect(ViewerPresenter.holderWords(snapshot) == "has the helper")
  }

  @Test("picking a desktop from the window gives its control back first, then shows only that desktop")
  func pickingADesktopGivesControlBackFirst() async throws {
    let service = ScriptedService()
    service.desktopList = DesktopLists.two(shown: "second-login")
    let viewer = try await ViewerController.watching(service)
    await viewer.actions.desktops.read()
    #expect(viewer.state.sidebar.helper.picksDesktop)
    await viewer.control.takeControl()
    #expect(viewer.control.lease == "lease")
    viewer.send(.view(desktop: "work"))
    try await settle("work was never shown") { viewer.actions.desktops.list?.viewed == "work" }
    #expect(viewer.control.lease == nil)
    let released = try #require(service.log.firstIndex { $0.hasPrefix("release") })
    let viewed = try #require(service.log.firstIndex(of: "view work"))
    #expect(released < viewed, "control on Second Login ends before work is shown")
    #expect(service.log.filter { $0.hasPrefix("view") } == ["view work"])
    #expect(viewer.state.sidebar.helper.desktops.first(where: \.isViewed)?.id == "work")
    #expect(viewer.desktopMoves == viewer.actions.desktops.moves, "the window followed its own pick")
    viewer.close()
  }

  @Test("a pick the service refuses says why and leaves the desktop shown as it was")
  func aRefusedPickSaysWhy() async throws {
    let service = ScriptedService()
    service.desktopList = DesktopLists.two(shown: "second-login")
    service.refusals["view"] = ServiceError("Atlas doesn't serve work.")
    let viewer = try await ViewerController.watching(service)
    await viewer.actions.desktops.read()
    await viewer.view("work")
    #expect(viewer.session.notice == "Atlas doesn't serve work.")
    #expect(viewer.actions.desktops.list?.viewed == "second-login")
    #expect(viewer.actions.desktops.moves == 0)
    viewer.close()
  }
}
