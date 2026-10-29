import AppKit
import AtlasUI
import CoreGraphics
import SwiftUI
import Testing

@testable import AtlasApp

@Suite("List motion frames", .serialized, AppKitState())
@MainActor
struct ListMotionFrameTests {
  @Test("a moment's title in the first frame of its fold stands where it stood, drawn as it was")
  func aFoldingHeaderStandsWhereItStood() throws {
    let pose = try #require(PreviewStates.viewerMotion.first { $0.name == "moment-fold" })
    let settled = try ViewerHost(pose.from, animatesArrivals: false)
      .brightness(at: Self.titlePoints)
    #expect(settled.contains { $0 < Self.ink }, "Notes' title is drawn where the frames are read")
    let first = try ViewerHost.firstFrame(from: pose.from, to: pose.to, animatesArrivals: false) {
      try $0.brightness(at: Self.titlePoints)
    }
    // Notes' title is drawn in the same place in the change's first frame: the header neither jumps nor is drawn
    // anew beside the one leaving.
    #expect(first == settled)
  }

  @Test(
    "opening a moment grows it in one motion, with the steps revealed in place and nothing below jumping",
    arguments: ["moment-open", "moment-fold"]
  )
  func aMomentOpensInOneMotion(name: String) throws {
    let pose = try #require(PreviewStates.viewerMotion.first { $0.name == name })
    let folder = FileManager.default.temporaryDirectory
      .appendingPathComponent("atlas-disclosure-\(UUID().uuidString)")
    try FileManager.default
      .createDirectory(at: folder, withIntermediateDirectories: true)
    defer {
      try? FileManager.default
        .removeItem(at: folder)
    }
    var edges: [CGFloat] = []
    var titles: [CGPoint] = []
    var typed: [CGPoint?] = []
    for step in pose.steps {
      #expect(ViewerRender.draw(pose, at: step, to: folder, appearance: Appearance.light))
      let frame = try Self.frame(folder.appendingPathComponent("\(pose.file(at: step)).png"))
      edges.append(try #require(Self.textEditFoot(in: frame), "TextEdit's row is drawn at \(step)"))
      titles.append(try #require(Self.inkCentre(of: frame, in: Self.titlePoints), "Notes' title is drawn at \(step)"))
      typed.append(Self.inkCentre(of: frame, in: Self.typedPoints))
    }
    // Notes' header stands still all the way, its words' ink in one place while its capsule fades under them.
    let title = try #require(titles.first)
    for centre in titles {
      #expect(hypot(centre.x - title.x, centre.y - title.y) <= Self.stillness, "Notes' title moved")
    }
    // The moment below moves one way, a little each tenth, never a jump: the disclosure's edge carries it.
    let travel = try #require(edges.last) - (try #require(edges.first))
    #expect(abs(travel) > Self.leastTravel, "the moment below moved with the disclosure")
    for (before, after) in zip(edges, edges.dropFirst()) {
      #expect((after - before) * travel >= 0, "the moment below turned back")
      #expect(abs(after - before) <= abs(travel) * Self.mostStep, "the moment below jumped")
    }
    // Notes' first step is revealed in place, not dropped or slid in: once the moment stands most of the way open, its
    // words are drawn where they rest when it is open.
    let opens = travel > 0
    let resting = try #require(opens ? typed.last ?? nil : typed.first ?? nil, "Notes' first step is drawn open")
    for (edge, centre) in zip(edges, typed) {
      let shown = (edge - (edges.first ?? edge)) / travel
      guard (opens ? shown : 1 - shown) >= Self.mostlyOpen else { continue }
      let drawn = try #require(centre, "Notes' first step is drawn while the moment is mostly open")
      // Down the list is where a drop or a slide would show.
      #expect(abs(drawn.y - resting.y) <= Self.stillness, "Notes' first step moved")
    }
  }

  @Test(
    "held disclosure frames draw identical pixels when repeated in reverse order after the real clock advances",
    arguments: ["moment-open", "moment-fold"]
  )
  func heldDisclosureFramesDrawIdenticalPixels(name: String) throws {
    let pose = try #require(PreviewStates.viewerMotion.first { $0.name == name })
    let folder = FileManager.default.temporaryDirectory
      .appendingPathComponent("atlas-repeated-disclosure-\(UUID().uuidString)")
    try FileManager.default
      .createDirectory(at: folder, withIntermediateDirectories: true)
    defer {
      try? FileManager.default
        .removeItem(at: folder)
    }
    var frames: [Double: Data] = [:]
    for step in pose.steps {
      #expect(ViewerRender.draw(pose, at: step, to: folder, appearance: Appearance.light))
      frames[step] = try Data(contentsOf: folder.appendingPathComponent("\(pose.file(at: step)).png"))
    }
    RunLoop.main
      .run(until: Date(timeIntervalSinceNow: ViewerMotionPose.tick))
    for step in pose.steps.reversed() {
      #expect(ViewerRender.draw(pose, at: step, to: folder, appearance: Appearance.light))
      let repeated = try Data(contentsOf: folder.appendingPathComponent("\(pose.file(at: step)).png"))
      #expect(repeated == frames[step], "\(name) at \(step) changed pixels")
    }
  }

  /// A key frame's pixels.
  ///
  /// - Parameter file: The PNG.
  /// - Returns: Its bitmap.
  /// - Throws: When it cannot be read.
  static func frame(_ file: URL) throws -> NSBitmapImageRep {
    try #require(NSBitmapImageRep(data: Data(contentsOf: file)))
  }

  /// How bright a key frame is at `point`, in the window's points.
  ///
  /// - Parameters:
  ///   - frame: The key frame.
  ///   - point: The point.
  /// - Returns: Its brightness, from 0 to 1, or 0 when nothing was drawn there.
  static func brightness(of frame: NSBitmapImageRep, at point: CGPoint) -> CGFloat {
    let scale = CGFloat(frame.pixelsWide) / PreviewStates.viewerSize.width
    return frame.colorAt(x: Int(point.x * scale), y: Int(point.y * scale))?
      .usingColorSpace(.sRGB)?
      .brightnessComponent ?? .zero
  }

  /// Where words' ink stands among `points`, weighted by contrast against their ground. The ink cutoff only decides
  /// whether words are visible: subtracting it from every sample discards different antialiased edges as words fade,
  /// moving the measured centre even when every glyph stands still. Contrast keeps those edges and scales with a fade.
  ///
  /// - Parameters:
  ///   - frame: The key frame.
  ///   - points: The points the words stand among.
  /// - Returns: The centre, or nil when no ink is drawn there.
  static func inkCentre(of frame: NSBitmapImageRep, in points: [CGPoint]) -> CGPoint? {
    let shades = points.map { ($0, brightness(of: frame, at: $0)) }
    guard shades.contains(where: { $0.1 < ink }) else { return nil }
    let ground =
      shades.map(\.1)
      .max() ?? .zero
    let weighed = shades.map { ($0.0, max(ground - $0.1, .zero)) }
    let total = weighed.reduce(.zero) { $0 + $1.1 }
    guard total > .zero else { return nil }
    let x = weighed.reduce(.zero) { $0 + $1.0.x * $1.1 } / total
    let y = weighed.reduce(.zero) { $0 + $1.0.y * $1.1 } / total
    return CGPoint(x: x, y: y)
  }

  /// Where TextEdit's folded capsule ends at its foot, down a line through it past its words: the first point back on
  /// the list's ground after the capsule, below Notes' header.
  ///
  /// - Parameter frame: The key frame.
  /// - Returns: The foot, in the window's points from its top, or nil when no capsule is found.
  static func textEditFoot(in frame: NSBitmapImageRep) -> CGFloat? {
    let ground = brightness(of: frame, at: CGPoint(x: footColumn, y: groundRow))
    var isInCapsule = false
    for row in stride(from: footSearch, to: PreviewStates.viewerSize.height, by: 1) {
      let shade = brightness(of: frame, at: CGPoint(x: footColumn, y: row))
      if abs(shade - ground) > Self.shadeStep {
        isInCapsule = true
      } else if isInCapsule {
        return row
      }
    }
    return nil
  }

  /// 1100: a line down through the folded capsules, past their words and short of their times and the small screen.
  static let footColumn: CGFloat = 1100
  /// 300: a point on that line in Opie's open steps, where only the list's ground is drawn.
  static let groundRow: CGFloat = 300
  /// 470: where the search for TextEdit's capsule starts, under Notes' header.
  static let footSearch: CGFloat = 470
  /// 40: the least the moment below must travel, a step's height or more.
  static let leastTravel: CGFloat = 40
  /// Points across and down the words of Notes' first step, "Typed", where they rest while it is open.
  static let typedPoints = (464...478)
    .flatMap { y in points(across: 286...330, down: y) }
  /// 0.6: how far open a moment stands, as a share of its travel, by when its first step is wholly revealed.
  static let mostlyOpen: CGFloat = 0.6
  /// 0.25: how far a title may seem to move from rounding alone, a quarter point.
  static let stillness: CGFloat = 0.25
  /// 0.01: how far apart two shades must be to read as a folded capsule against the list's ground.
  static let shadeStep: CGFloat = 0.01
  /// 0.2: the most of its travel the moment below may move between two tenths, twice an even tenth's share.
  static let mostStep: CGFloat = 0.2

  /// Points across and down Notes' title in its header, under Opie's open steps, every point from 286 to 328 across
  /// and 430 to 444 down.
  static let titlePoints = (430...444)
    .flatMap { y in points(across: 286...328, down: y) }
  /// 0.5: darker than this is the title's ink, not the list's ground.
  static let ink: CGFloat = 0.5

  /// Every point across a span at one height.
  ///
  /// - Parameters:
  ///   - xs: The span across.
  ///   - y: The height.
  /// - Returns: The points, from left to right.
  static func points(across xs: ClosedRange<Int>, down y: Int) -> [CGPoint] {
    xs.map { CGPoint(x: $0, y: y) }
  }
}
