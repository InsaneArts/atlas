import AppKit
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Reduced layout geometry", .serialized, AppKitState())
@MainActor
struct ReduceMotionGeometryTests {
  @Test("the footer age takes its final position immediately when reduced motion hides the action words")
  func footerWordsCommitGeometryAtOnce() throws {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: -5)
    defer { Motion.Pose.hold(at: previous) }
    var first = PreviewStates.viewerWorking
    first.footer.fit = .compact
    var next = first
    next.footer.fit = .tight
    let host = ViewerHost(first, animatesArrivals: false, reduceMotion: true)
    host.render()
    let initial = try #require(host.frames["footer age"])
    Motion.Pose.hold(at: .zero)
    host.show(next)
    var frames: [CGRect] = []
    for time in [0.0, 0.04, 0.08, 0.15, 1.0] {
      Motion.Pose.hold(at: time)
      host.render()
      frames.append(try #require(host.frames["footer age"]))
    }
    let final = try #require(frames.last)
    #expect(initial != final)
    #expect(frames.allSatisfy { $0 == final }, "opacity changes must never move the age")
  }

  @Test("narrowing the list places surviving runs at their final geometry throughout the opacity fade")
  func narrowingCommitsGeometryAtOnce() throws {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: -5)
    defer { Motion.Pose.hold(at: previous) }
    let first = PreviewStates.viewerActivity
    var next = first
    var activity = try #require({ if case .activity(let state) = first.pane { state } else { nil } }())
    let runs = activity.days[0].sessions[0].runs
    let chromeIndex = try #require(runs.firstIndex { $0.name == "Google Chrome" })
    let chrome = runs[chromeIndex]
    activity.days[0].sessions[0].runs = Array(runs[chromeIndex...])
    next.pane = .activity(activity)
    let host = ViewerHost(first, animatesArrivals: false, reduceMotion: true)
    host.render()
    Motion.Pose.hold(at: .zero)
    host.render()
    let initial = try #require(host.frames["run \(chrome.id)"])
    host.show(next)
    var frames: [CGRect] = []
    for time in [0.0, 0.04, 0.08, 0.15, 1.0] {
      Motion.Pose.hold(at: time)
      host.render()
      frames.append(try #require(host.frames["run \(chrome.id)"]))
    }
    let final = try #require(frames.last)
    #expect(initial != final, "the surviving Chrome run actually changes position")
    #expect(frames.allSatisfy { $0 == final }, "no intermediate layout is permitted while rows fade")
  }

  @Test("the compact sidebar footer, a row's height like the whole one, never moves the sessions' room as it fades")
  func compactFooterCommitsGeometryAtOnce() throws {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: -5)
    defer { Motion.Pose.hold(at: previous) }
    let first = PreviewStates.viewerSessions
    var next = first
    next.sidebar.isShort.toggle()
    let host = ViewerHost(first, animatesArrivals: false, reduceMotion: true)
    host.render()
    let list = try #require(host.sidebarScrollViews.first)
    let initial = host.frameFromTop(of: list)
    Motion.Pose.hold(at: .zero)
    host.show(next)
    var frames: [CGRect] = []
    for time in [0.0, 0.04, 0.08, 0.15, 1.0] {
      Motion.Pose.hold(at: time)
      host.render()
      frames.append(host.frameFromTop(of: list))
    }
    #expect(frames.allSatisfy { $0 == initial }, "the sessions' room must never move")
  }
}
