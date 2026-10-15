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

/// A viewer service that answers from fixed replies, can hold its status
/// reply until told, and records what it was asked.
@MainActor
final class ScriptedService: ViewerService, @unchecked Sendable {
    nonisolated let base = URL(string: "http://127.0.0.1:1/x/")!
    var reply: [String: Any] = ["ready": true, "canObserve": true, "overview": ["mode": "observing"]]
    var revision = "r1"
    var picture = Data()
    var holdStatus = false
    var failStatus = false
    var held: CheckedContinuation<Void, Never>?
    var statusCalls = 0
    var log: [String] = []

    func status() async throws -> ViewerStatus {
        statusCalls += 1
        if holdStatus {
            holdStatus = false
            await withCheckedContinuation { held = $0 }
        }
        if failStatus { throw ServiceError("late network error") }
        return try JSONDecoder().decode(ViewerStatus.self, from: JSONSerialization.data(withJSONObject: reply))
    }

    func frame() async throws -> Frame {
        Frame(png: picture, id: "frame-\(statusCalls)", revision: revision, display: .init(width: 1512, height: 982))
    }

    func acquire(frame: String) async throws -> String {
        log.append("acquire \(frame)")
        return "lease"
    }
    func renew(lease: String, frame: String) async throws { log.append("renew") }
    func release(lease: String) async throws { log.append("release \(lease)") }
    func revoke() async throws { log.append("revoke") }
    func input(lease: String, frame: String, actions: [InputAction]) async throws {
        log.append("input \(lease) \(actions.count)")
    }
    func recover() async throws -> Bool {
        log.append("recover")
        return true
    }
}

@MainActor
@Suite struct ViewerModelTests {
    func settle(_ done: () -> Bool = { false }) async {
        for _ in 0..<100 where !done() { try? await Task.sleep(for: .milliseconds(5)) }
    }

    func watching(_ service: ScriptedService) async -> ViewerModel {
        service.picture = png()
        let model = ViewerModel(service: service)
        // A fixed clock: pictures never age, however slowly the tests run.
        model.clock = { 0 }
        model.visible(true)
        await settle { model.frame != nil }
        return model
    }

    @Test func aReadingShowsTheStatusAndThePicture() async {
        let service = ScriptedService()
        let model = await watching(service)
        #expect(model.frame != nil)
        #expect(model.image != nil)
        #expect(model.ready)
        #expect(model.mode.label == "Watching")
        #expect(model.canTakeControl)
        model.close()
    }

    @Test func theMenuBarsPictureShowsAtOnceButControlWaitsForAReading() async throws {
        let service = ScriptedService()
        service.picture = png()
        let model = ViewerModel(service: service)
        model.clock = { 0 }
        let status = try JSONDecoder().decode(
            ViewerStatus.self, from: Data(#"{"ready": true, "overview": {"mode": "observing"}}"#.utf8))
        model.begin(with: status, image: NSImage(data: png()))
        #expect(model.image != nil)
        #expect(!model.canTakeControl, "a picture from the menu bar names no frame")
        model.visible(true)
        await settle { model.frame != nil }
        #expect(model.canTakeControl)
        model.close()
    }

    @Test func pausingDropsAReadingStillUnderWayAndResumingReadsOnce() async {
        let service = ScriptedService()
        service.holdStatus = true
        service.picture = png()
        let model = ViewerModel(service: service)
        model.visible(true)
        await settle { service.held != nil }
        model.togglePause()
        model.togglePause()
        model.togglePause()
        service.held?.resume()
        await settle()
        #expect(model.frame == nil, "the paused window shows no late picture")
        #expect(model.paused)
        model.togglePause()
        await settle { model.frame != nil }
        #expect(model.frame != nil)
        model.close()
    }

    @Test func aLateFailureCannotOverwriteAPausedViewer() async {
        let service = ScriptedService()
        service.holdStatus = true
        service.failStatus = true
        let model = ViewerModel(service: service)
        model.visible(true)
        await settle { service.held != nil }
        model.togglePause()
        service.held?.resume()
        await settle()
        #expect(model.unavailable == nil)
        model.close()
    }

    @Test func aStatusThatCannotObserveSaysWhy() async {
        let service = ScriptedService()
        service.reply = ["canObserve": false, "nextSteps": ["Sign in to the helper."]]
        let model = ViewerModel(service: service)
        model.visible(true)
        await settle { model.unavailable != nil }
        #expect(model.unavailable == "Sign in to the helper.")
        #expect(model.mode.label == "Reconnecting")
        model.close()
    }

    @Test func takingControlHoldsALeaseAndGivingItBackReleasesIt() async {
        let service = ScriptedService()
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
        let service = ScriptedService()
        let model = await watching(service)
        await model.takeControl()
        model.lostFocus()
        #expect(model.lease == nil)
        model.close()
    }

    @Test func aGrantThatArrivesAfterLeavingIsGivenBack() async {
        let service = ScriptedService()
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

    @Test func controlCanBeTakenAgainAfterComingBackToTheWindow() async {
        let service = ScriptedService()
        let model = await watching(service)
        await model.takeControl()
        model.lostFocus()
        model.gainedFocus()
        await model.takeControl()
        #expect(model.lease == "lease")
        model.close()
    }

    @Test func aStatusThatIsNotReadyEndsControlAtOnce() async {
        let service = ScriptedService()
        let model = await watching(service)
        await model.takeControl()
        service.reply = ["ready": false, "canObserve": true, "overview": ["mode": "observing"]]
        await model.readStatus()
        #expect(model.lease == nil)
        #expect(model.notice == "Mouse and keyboard permission is unavailable.")
        model.close()
    }

    @Test func hidingTheWindowGivesControlBack() async {
        let service = ScriptedService()
        let model = await watching(service)
        await model.takeControl()
        model.visible(false)
        #expect(model.lease == nil)
        model.close()
    }

    @Test func aStalePictureEndsControlWithoutFurtherInput() async {
        let service = ScriptedService()
        let model = await watching(service)
        await model.takeControl()
        model.setActive(false)
        model.clock = { 10 }
        await settle { model.lease == nil }
        #expect(model.lease == nil)
        #expect(model.notice == "The picture stopped updating. Wait for a fresh one, then choose Control.")
        model.close()
    }

    @Test func aChangedHelperEndsControl() async {
        let service = ScriptedService()
        let model = await watching(service)
        await model.takeControl()
        service.revision = "r2"
        model.refresh()
        await settle { model.lease == nil }
        #expect(model.lease == nil)
        #expect(model.notice == "The helper changed. Choose Control again when ready.")
        model.close()
    }

    @Test func controlWaitsWhileAnAgentWorks() async {
        let service = ScriptedService()
        service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "agent"]]
        let model = await watching(service)
        #expect(!model.canTakeControl)
        #expect(model.hint == "An agent is acting in the helper.")
        model.close()
    }

    @Test func typedTextAndKeysGoToTheHelperWhileInControl() async {
        let service = ScriptedService()
        let model = await watching(service)
        await model.takeControl()
        model.send(text: "héllo")
        #expect(model.keyPressed(keyCode: 36, modifiers: []) { nil })
        #expect(!model.keyPressed(keyCode: 14, modifiers: []) { nil }, "plain letters are text")
        await settle { service.log.filter { $0.hasPrefix("input") }.count >= 2 }
        #expect(service.log.filter { $0.hasPrefix("input lease") }.count == 2)
        #expect(model.keyPressed(keyCode: 53, modifiers: [.control, .shift]) { nil })
        #expect(model.lease == nil, "Control-Shift-Esc gives control back")
        model.close()
    }

    @Test func pasteIsLimited() async {
        let service = ScriptedService()
        let model = await watching(service)
        await model.takeControl()
        model.send(text: String(repeating: "a", count: 2049))
        #expect(model.notice == "Paste is limited to 2,048 characters at a time.")
        model.close()
    }

    @Test func recoveryCanBeSetAsideUntilItsReasonChanges() async {
        let service = ScriptedService()
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
        #expect(model.notice == "The helper was recovered. Choose Control again when ready.")
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
