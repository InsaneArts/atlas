import AppKit
import AsyncSupportTesting
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Panel action swaps", .serialized, AppKitState())
@MainActor
struct PanelActionSwapTests {
  @Test(
    "Stop and Resume reject activation throughout interrupted panel label replacements",
    arguments: [false, true]
  )
  func agentActionsFollowTheirSettledLabels(reduced: Bool) async {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: -5)
    defer { Motion.Pose.hold(at: previous) }
    let host = PanelButtonHost(reduceMotion: reduced)
    var controls = PreviewStates.panelWorking.controls
    controls.main.isEnabled = false
    host.show(controls)
    await Self.render(host)
    #expect(host.press() == [.stopAgent])
    Motion.Pose.hold(at: .zero)
    controls.agent = .resume(isEnabled: true)
    host.show(controls)
    await Self.render(host)
    #expect(host.press().isEmpty)
    Motion.Pose.hold(at: 0.04)
    controls.agent = .stop
    host.show(controls)
    await Self.render(host)
    #expect(host.press().isEmpty)
    await Self.settle(host, from: 0.04)
    #expect(host.press() == [.stopAgent])
    controls.agent = .resume(isEnabled: true)
    host.show(controls)
    await Self.render(host)
    #expect(host.press().isEmpty)
    await Self.settle(host, from: 0.54)
    #expect(host.press() == [.resumeAgent])
    controls.agent = .resume(isEnabled: false)
    host.show(controls)
    await Self.render(host)
    #expect(host.press().isEmpty)
  }

  @Test(
    "the panel main button dispatches only its settled label's action after interrupted replacements",
    arguments: [false, true]
  )
  func mainActionsFollowTheirSettledLabels(reduced: Bool) async {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: -5)
    defer { Motion.Pose.hold(at: previous) }
    let host = PanelButtonHost(reduceMotion: reduced)
    var first = PreviewStates.panelIdle.controls
    first.agent = nil
    var next = PreviewStates.panelNotReady.controls
    next.agent = nil
    host.show(first)
    await Self.render(host)
    #expect(host.press() == [first.main.event])
    Motion.Pose.hold(at: .zero)
    host.show(next)
    await Self.render(host)
    #expect(host.press().isEmpty)
    Motion.Pose.hold(at: 0.04)
    host.show(first)
    await Self.render(host)
    #expect(host.press().isEmpty)
    await Self.settle(host, from: 0.04)
    #expect(host.press() == [first.main.event])
    host.show(next)
    await Self.render(host)
    #expect(host.press().isEmpty)
    await Self.settle(host, from: 0.54)
    #expect(host.press() == [next.main.event])
    next.main.isEnabled = false
    host.show(next)
    await Self.render(host)
    #expect(host.press().isEmpty)
  }

  @Test("panel action labels change without moving or resizing either capsule", arguments: [false, true])
  func labelReplacementsKeepTheirGeometry(reduced: Bool) async throws {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: -5)
    defer { Motion.Pose.hold(at: previous) }
    let host = PanelButtonHost(reduceMotion: reduced)
    let first = PreviewStates.panelWorking.controls
    var next = first
    next.agent = .resume(isEnabled: true)
    next.main = PreviewStates.panelNotReady.controls.main
    host.show(first)
    await Self.render(host)
    let agent = try #require(host.frames["panel agent action"])
    let main = try #require(host.frames["panel main action"])
    #expect(agent.width > 0 && main.width > 0)
    Motion.Pose.hold(at: .zero)
    host.show(next)
    for time in [0.0, 0.02, 0.04, 0.08, 0.12, 0.2, 0.4, 1.0] {
      Motion.Pose.hold(at: time)
      if time == 0.04 { host.show(first) }
      await Self.render(host)
      #expect(host.frames["panel agent action"] == agent)
      #expect(host.frames["panel main action"] == main)
    }
  }

  private static func render(_ host: PanelButtonHost) async {
    for _ in 0..<6 {
      await afterQueuedWork()
      host.render()
    }
  }

  private static func settle(_ host: PanelButtonHost, from start: TimeInterval) async {
    for step in 1...10 {
      Motion.Pose.hold(at: start + Double(step) * 0.05)
      await render(host)
    }
  }
}
