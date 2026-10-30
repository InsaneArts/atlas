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

  @Test("filtering places surviving moments at their final geometry throughout the opacity fade")
  func filteringCommitsGeometryAtOnce() throws {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: -5)
    defer { Motion.Pose.hold(at: previous) }
    let first = PreviewStates.viewerActivity
    var next = first
    var activity = try #require({ if case .activity(let state) = first.pane { state } else { nil } }())
    let notesIndex = try #require(
      activity.days[0].moments.firstIndex { $0.title == "Notes" }
    )
    let notes = activity.days[0].moments[notesIndex]
    activity.days[0].moments = Array(activity.days[0].moments[notesIndex...])
    next.pane = .activity(activity)
    let host = ViewerHost(first, animatesArrivals: false, reduceMotion: true)
    host.render()
    Motion.Pose.hold(at: .zero)
    host.render()
    let initial = try #require(host.frames["moment \(notes.id)"])
    withAnimation(ActivityMotion.filter(reduceMotion: true)) { host.show(next) }
    var frames: [CGRect] = []
    for time in [0.0, 0.04, 0.08, 0.15, 1.0] {
      Motion.Pose.hold(at: time)
      host.render()
      frames.append(try #require(host.frames["moment \(notes.id)"]))
    }
    let final = try #require(frames.last)
    #expect(initial != final, "the surviving Notes moment actually changes position")
    #expect(frames.allSatisfy { $0 == final }, "no intermediate layout is permitted while rows fade")
  }

  @Test("the compact sidebar footer gives the sessions their final room throughout its opacity fade")
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
    let final = try #require(frames.last)
    #expect(initial != final, "the footer actually changes the sessions' available height")
    #expect(frames.allSatisfy { $0 == final }, "the sessions' room must never interpolate")
  }
}
