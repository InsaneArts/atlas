import AppKit
import AtlasKit
import Foundation
import Testing

@testable import AtlasUI

/// A tiny PNG, as the service sends.
@MainActor
func png(width: Int = 4, height: Int = 3) -> Data {
    let bitmap = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height, bitsPerSample: 8, samplesPerPixel: 4,
        hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)
    return bitmap?.representation(using: .png, properties: [:]) ?? Data()
}

@MainActor
@Suite struct ViewerModelTests {
    func watching(_ service: FakeService) async -> ViewerModel {
        service.picture = png()
        let model = ViewerModel(service: service)
        // A fixed clock: pictures never age, however slowly the tests run.
        model.clock = { 0 }
        model.visible(true)
        await settle { model.frame != nil }
        return model
    }

    @Test func aReadingShowsTheStatusAndThePicture() async {
        let service = FakeService()
        let model = await watching(service)
        #expect(model.frame != nil)
        #expect(model.image != nil)
        #expect(model.ready)
        #expect(model.mode.label == "Watching")
        #expect(model.canTakeControl)
        model.close()
    }

    @Test func thePanelsPictureShowsAtOnceButControlWaitsForAReading() async throws {
        let service = FakeService()
        service.picture = png()
        let model = ViewerModel(service: service)
        model.clock = { 0 }
        let status = try JSONDecoder().decode(
            ViewerStatus.self, from: Data(#"{"ready": true, "overview": {"mode": "observing"}}"#.utf8))
        model.begin(with: status, image: NSImage(data: png()))
        #expect(model.image != nil)
        #expect(!model.canTakeControl, "a picture from the panel names no frame")
        model.visible(true)
        await settle { model.frame != nil }
        #expect(model.canTakeControl)
        model.close()
    }

    @Test func pausingDropsAReadingStillUnderWayAndResumingReadsOnce() async {
        let service = FakeService()
        service.hold = ["status"]
        service.picture = png()
        let model = ViewerModel(service: service)
        model.visible(true)
        await settle { service.held["status"] != nil }
        model.togglePause()
        model.togglePause()
        model.togglePause()
        service.resume("status")
        await settle { service.answered == 1 }
        #expect(model.frame == nil, "the paused window shows no late picture")
        #expect(model.paused)
        model.togglePause()
        await settle { model.frame != nil }
        #expect(model.frame != nil)
        model.close()
    }

    @Test func aLateFailureCannotOverwriteAPausedViewer() async {
        let service = FakeService()
        service.hold = ["status"]
        service.fail = ["status"]
        let model = ViewerModel(service: service)
        model.visible(true)
        await settle { service.held["status"] != nil }
        model.togglePause()
        service.resume("status")
        await settle { service.answered == 1 }
        #expect(model.unavailable == nil)
        model.close()
    }

    @Test func aStatusThatCannotObserveSaysWhy() async {
        let service = FakeService()
        service.reply = ["canObserve": false, "nextSteps": ["Sign in to the helper."]]
        let model = ViewerModel(service: service)
        model.visible(true)
        await settle { model.unavailable != nil }
        #expect(model.unavailable == "Sign in to the helper.")
        #expect(model.mode.label == "Reconnecting")
        model.close()
    }

    @Test func takingControlHoldsALeaseAndGivingItBackReleasesIt() async {
        let service = FakeService()
        let model = await watching(service)
        await model.takeControl()
        #expect(model.lease == "lease")
        #expect(model.mode.label == "You have control")
        #expect(service.log.contains { $0.hasPrefix("acquire frame-") })
        model.releaseControl()
        #expect(model.lease == nil)
        await settle { service.log.contains("release lease") }
        #expect(service.log.contains("release lease"))
        model.close()
    }

    @Test func leavingTheWindowGivesControlBack() async {
        let service = FakeService()
        let model = await watching(service)
        await model.takeControl()
        model.lostFocus()
        #expect(model.lease == nil)
        model.close()
    }

    @Test func aGrantThatArrivesAfterLeavingIsGivenBack() async {
        let service = FakeService()
        let model = await watching(service)
        model.gainedFocus()
        let taking = Task { await model.takeControl() }
        model.lostFocus()
        await taking.value
        #expect(model.lease == nil)
        await settle { service.log.contains("release lease") }
        #expect(service.log.contains("release lease"))
        model.close()
    }

    @Test(arguments: [true, false])
    func aGrantThatArrivesWhileHiddenOrPausedIsGivenBack(hidden: Bool) async {
        let service = FakeService()
        let model = await watching(service)
        service.hold = ["acquire"]
        let taking = Task { await model.takeControl() }
        await settle { service.held["acquire"] != nil }
        #expect(model.acquiring)
        if hidden { model.visible(false) } else { model.togglePause() }
        service.resume("acquire")
        await taking.value
        #expect(model.lease == nil)
        #expect(!model.acquiring)
        await settle { service.log.contains("release lease") }
        model.close()
    }

    @Test func controlCanBeTakenAgainAfterComingBackToTheWindow() async {
        let service = FakeService()
        let model = await watching(service)
        await model.takeControl()
        model.lostFocus()
        model.gainedFocus()
        await model.takeControl()
        #expect(model.lease == "lease")
        model.close()
    }

    @Test func aStatusThatIsNotReadyEndsControlAtOnce() async {
        let service = FakeService()
        let model = await watching(service)
        await model.takeControl()
        service.reply = ["ready": false, "canObserve": true, "overview": ["mode": "observing"]]
        await model.readStatus()
        #expect(model.lease == nil)
        #expect(model.notice == "Mouse and keyboard permission is unavailable.")
        model.close()
    }

    @Test func hidingTheWindowGivesControlBack() async {
        let service = FakeService()
        let model = await watching(service)
        await model.takeControl()
        model.visible(false)
        #expect(model.lease == nil)
        model.close()
    }

    @Test func aStalePictureEndsControlWithoutFurtherInput() async {
        let service = FakeService()
        let model = await watching(service)
        model.heartbeatInterval = .milliseconds(5)
        await model.takeControl()
        model.setActive(false)
        model.clock = { Freshness.stale }
        #expect(!model.canInput)
        await settle { model.lease == nil }
        #expect(model.notice == ViewerModel.stalePicture)
        model.close()
    }

    @Test func aFailedRenewalEndsControl() async {
        let service = FakeService()
        let model = await watching(service)
        model.heartbeatInterval = .milliseconds(5)
        service.fail = ["renew"]
        await model.takeControl()
        await settle { model.lease == nil }
        #expect(service.log.contains("renew"))
        #expect(model.notice == "renew failed")
        model.close()
    }

    @Test func anInputThatFailsEndsControl() async {
        let service = FakeService()
        let model = await watching(service)
        service.fail = ["input"]
        await model.takeControl()
        model.sendKey("escape")
        await settle { model.lease == nil }
        #expect(model.notice == "input failed")
        model.close()
    }

    @Test func aChangedHelperEndsControl() async {
        let service = FakeService()
        let model = await watching(service)
        await model.takeControl()
        service.revision = "r2"
        model.refresh()
        await settle { model.lease == nil }
        #expect(model.lease == nil)
        #expect(model.notice == "The helper changed. Take control again when ready.")
        model.close()
    }

    @Test func controlWaitsWhileAnAgentWorks() async {
        let service = FakeService()
        service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "agent"]]
        let model = await watching(service)
        #expect(!model.canTakeControl)
        #expect(model.hint == "An agent is acting in the helper.")
        model.close()
    }

    @Test func typedTextAndKeysGoToTheHelperWhileInControl() async {
        let service = FakeService()
        let model = await watching(service)
        await model.takeControl()
        model.send(text: "héllo")
        #expect(model.keyPressed(keyCode: 36, modifiers: []) { nil })
        #expect(!model.keyPressed(keyCode: 14, modifiers: []) { nil }, "plain letters are text")
        await settle { service.log.filter { $0.hasPrefix("input") }.count >= 2 }
        #expect(service.log.filter { $0.hasPrefix("input lease") }.count == 2)
        #expect(model.keyPressed(keyCode: 53, modifiers: [.control, .shift]) { nil })
        #expect(model.lease == nil, "Control-Shift-Escape gives control back")
        model.close()
    }

    @Test func scrollingIsGatheredIntoOneBatch() async {
        let service = FakeService()
        let model = await watching(service)
        await model.takeControl()
        let size = CGSize(width: 400, height: 300)
        for _ in 0..<3 { model.scrolled(dx: 0, dy: 10, precise: false, at: CGPoint(x: 200, y: 150), in: size) }
        await settle { service.actions.count == 1 }
        #expect(service.actions.first?.count == 3)
        #expect(service.actions.first?.last == .scroll(x: 756, y: 491, dx: 0, dy: -30))
        model.close()
    }

    @Test func composedTextIsShownUntilCommittedThenTyped() async {
        let service = FakeService()
        let model = await watching(service)
        await model.takeControl()
        let canvas = DesktopCanvasView()
        canvas.model = model
        let none = NSRange(location: NSNotFound, length: 0)
        canvas.setMarkedText("ni", selectedRange: NSRange(location: 2, length: 0), replacementRange: none)
        #expect(canvas.hasMarkedText())
        #expect(model.composing == "ni")
        canvas.unmarkText()
        #expect(!canvas.hasMarkedText())
        #expect(model.composing.isEmpty)
        canvas.setMarkedText("n", selectedRange: NSRange(location: 1, length: 0), replacementRange: none)
        canvas.insertText("你", replacementRange: none)
        #expect(!canvas.hasMarkedText())
        #expect(model.composing.isEmpty)
        await settle { service.actions.count == 1 }
        #expect(service.actions.first == [.type(text: "你")])
        model.close()
    }

    @Test func theKeysMenuSendsTheKeyItNames() async throws {
        let service = FakeService()
        let model = await watching(service)
        await model.takeControl()
        let menu = KeysButton.menu(for: model)
        let items = menu.items.compactMap { $0 as? KeyItem }
        #expect(items.count == KeysButton.groups.joined().count)
        let switchApp = try #require(items.first { $0.title == "Switch App" })
        #expect(switchApp.keyEquivalent == "\t" && switchApp.keyEquivalentModifierMask == .command)
        switchApp.fire()
        try #require(items.first { $0.title == "Escape" }).fire()
        await settle { service.actions.count == 2 }
        #expect(service.actions == [[.key("cmd+tab")], [.key("escape")]])
        model.releaseControl()
        switchApp.fire()
        try? await Task.sleep(for: .milliseconds(50))
        #expect(service.actions.count == 2, "nothing is sent without control")
        model.close()
    }

    @Test func captionsStepAsideForTheSidebarControlAndPause() async {
        let service = FakeService()
        service.reply = [
            "ready": true, "canObserve": true,
            "overview": ["mode": "observing", "activity": [["operation": "run", "ok": true, "ageSeconds": 1]]],
        ]
        let model = await watching(service)
        #expect(model.captions.map(\.line) == ["Agent ran a command"])
        model.sidebarShown = true
        #expect(model.captions.isEmpty)
        model.sidebarShown = false
        await model.takeControl()
        #expect(model.captions.isEmpty)
        model.close()
    }

    @Test func pasteIsLimited() async {
        let service = FakeService()
        let model = await watching(service)
        await model.takeControl()
        model.send(text: String(repeating: "a", count: 2049))
        #expect(model.notice == "Paste is limited to 2,048 characters at a time.")
        model.close()
    }

    @Test func recoveryCanBeSetAsideUntilItsReasonChanges() async {
        let service = FakeService()
        service.reply = [
            "ready": true, "canObserve": true, "overview": ["mode": "recovering", "reason": "Unknown outcome"],
        ]
        let model = await watching(service)
        #expect(model.recovery != nil)
        model.recoverLater()
        #expect(model.recovery == nil)
        #expect(model.recoveryDeferred)
        await model.recover()
        #expect(service.log.contains("recover"))
        #expect(model.notice == "The helper was recovered. Take control again when ready.")
        model.close()
    }
}

@MainActor
@Suite struct ViewerRenderTests {
    @Test func everyViewerStateDraws() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(
            "atlas-viewer-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        #expect(renderViewer(to: directory.path, size: NSSize(width: 900, height: 600)))
        let drawn = try FileManager.default.contentsOfDirectory(atPath: directory.path)
        #expect(drawn.count == viewerScreens().count)
    }
}
