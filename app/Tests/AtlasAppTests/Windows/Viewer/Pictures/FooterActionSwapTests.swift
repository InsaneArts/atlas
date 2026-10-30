import AppKit
import AsyncSupportTesting
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Footer action swaps", .serialized, AppKitState())
@MainActor
struct FooterActionSwapTests {
  @Test("Stop and Resume reject activation throughout interrupted label replacements")
  func agentActionsFollowTheirSettledLabels() async {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: -5)
    defer { Motion.Pose.hold(at: previous) }
    let host = FooterButtonHost()
    host.show(agent: .stop)
    await Self.render(host)
    #expect(host.press() == [.stopAgent])
    Motion.Pose.hold(at: .zero)
    host.show(agent: .resume(isEnabled: true))
    await Self.render(host)
    #expect(host.press().isEmpty)
    Motion.Pose.hold(at: 0.04)
    host.show(agent: .stop)
    await Self.render(host)
    #expect(host.press().isEmpty)
    await Self.settle(host, from: 0.04)
    #expect(host.press() == [.stopAgent])
    host.show(agent: .resume(isEnabled: true))
    await Self.render(host)
    #expect(host.press().isEmpty)
    await Self.settle(host, from: 0.54)
    #expect(host.press() == [.resumeAgent])
    host.show(agent: .resume(isEnabled: false))
    await Self.render(host)
    #expect(host.press().isEmpty)
  }

  @Test("the main button rejects activation during replacement and dispatches only the settled label's action")
  func mainActionsFollowTheirSettledLabels() async {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: -5)
    defer { Motion.Pose.hold(at: previous) }
    let host = FooterButtonHost()
    let take = PreviewStates.viewerTakeControl(isEnabled: true)
    let give = PreviewStates.viewerGiveBack
    host.show(control: take)
    await Self.render(host)
    #expect(host.press() == [take.event])
    Motion.Pose.hold(at: .zero)
    host.show(control: give)
    await Self.render(host)
    #expect(host.press().isEmpty)
    Motion.Pose.hold(at: 0.04)
    host.show(control: take)
    await Self.render(host)
    #expect(host.press().isEmpty)
    await Self.settle(host, from: 0.04)
    #expect(host.press() == [take.event])
    host.show(control: give)
    await Self.render(host)
    #expect(host.press().isEmpty)
    await Self.settle(host, from: 0.54)
    #expect(host.press() == [give.event])
  }

  private static func render(_ host: FooterButtonHost) async {
    for _ in 0..<6 {
      await afterQueuedWork()
      host.render()
    }
  }

  private static func settle(_ host: FooterButtonHost, from start: TimeInterval) async {
    for step in 1...10 {
      Motion.Pose.hold(at: start + Double(step) * 0.05)
      await render(host)
    }
  }
}
