import AtlasService
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// A step's small picture: the screen before its first action, marked as that action, with how many repeats it
  /// holds; nil for a step that kept no picture.
  ///
  /// - Parameters:
  ///   - run: The step's rows.
  ///   - snapshot: What the controller holds.
  /// - Returns: The picture, or nil.
  static func stepPicture(_ run: ActivityRun, _ snapshot: ViewerSnapshot) -> ViewerStepPicture? {
    guard let before = run.firstBefore else { return nil }
    let first = run.members.first ?? run.entry
    let uncertain = run.id == snapshot.reading.statusReading.uncertainRow
    return ViewerStepPicture(
      shot: activityShot(before, snapshot),
      mark: actionMark(first, repeats: run.count, tone: markTone(first, uncertain: uncertain)),
      repeats: run.count
    )
  }

  /// How an action's mark is coloured: as Atlas recorded its outcome beside its picture, else as its row went; the
  /// action of unknown outcome as uncertain.
  ///
  /// - Parameters:
  ///   - entry: The action's row.
  ///   - uncertain: Whether it is the action of unknown outcome.
  /// - Returns: The tone.
  static func markTone(_ entry: ActivityLog.Entry, uncertain: Bool) -> ViewerMarkTone {
    if uncertain { return .uncertain }
    switch entry.facts?.outcome {
    case .worked: return .worked
    case .failed: return .failed
    case .uncertain: return .uncertain
    case .notSent: return .notSent
    case nil:
      switch entry.outcome {
      case .failed: return .failed
      case .notSent: return .notSent
      case .running, .worked: return .worked
      }
    }
  }

  /// An action's mark, from what Atlas recorded beside its picture: a click's ring and dots, typing's ring and what was
  /// typed (eight dots for a password, never its text or length), keys as keycaps, a scroll's pill the way the page
  /// moved with its count, a drag's path, or a press's ring; where on the picture from its point on the display, the
  /// ring around the element it went to where its frame is known (never for a scroll or a drag, whose marks stand at
  /// their points), and for one that did not work, why. Nil when nothing was recorded.
  ///
  /// - Parameters:
  ///   - entry: The action's row.
  ///   - repeats: How many actions its step holds, which a scroll's tag counts.
  ///   - tone: How it went.
  /// - Returns: The mark, or nil.
  static func actionMark(_ entry: ActivityLog.Entry, repeats: Int, tone: ViewerMarkTone) -> ViewerActionMark? {
    guard let facts = entry.facts, let action = facts.action else { return nil }
    let shape: ViewerMarkShape
    switch action {
    case .click:
      shape = .click(clicks: facts.clicks ?? 1)
    case .type:
      shape =
        facts.secure
        ? .typing(text: ViewerWords.passwordDots, detail: ViewerWords.passwordDetail)
        : .typing(text: facts.text ?? "", detail: ViewerWords.typedDetail(facts.length))
    case .key:
      let keys = facts.keys.isEmpty ? entry.keys : facts.keys
      shape = .keys(keys, name: ViewerWords.keyName(keys))
    case .scroll:
      let arrow = scrollArrow(facts.scroll)
      shape = .scroll(arrow: arrow, label: ViewerWords.scrollTag(arrow, count: repeats))
    case .drag:
      if let to = facts.to.flatMap({ share($0, of: facts.display) }) {
        shape = .drag(to: to)
      } else {
        shape = .press
      }
    case .press, .move:
      shape = .press
    }
    let said = facts.reason ?? entry.reason
    let reason = said.map(Words.leadingCapital)
    let area: CGRect? =
      switch shape {
      case .scroll, .drag: nil
      default: facts.element.flatMap { share($0.frame, of: facts.display) }
      }
    return ViewerActionMark(
      shape: shape,
      spot: facts.point.flatMap { share($0, of: facts.display) },
      area: area,
      tone: tone,
      note: tone == .worked ? nil : reason ?? (tone == .uncertain ? ViewerWords.mayNotHaveFinished : nil)
    )
  }

  /// The way a scroll moved the page: along its longer reach, down when it says none.
  ///
  /// - Parameter scroll: How far it scrolled.
  /// - Returns: The way.
  static func scrollArrow(_ scroll: ActionFacts.Scroll?) -> ViewerScrollArrow {
    guard let scroll else { return .down }
    if abs(scroll.dx) > abs(scroll.dy) { return scroll.dx > 0 ? .right : .left }
    return scroll.dy < 0 ? .up : .down
  }

  /// An element's frame on the display as shares of its width and height, kept on the picture; nil when none of it
  /// is on the picture.
  ///
  /// - Parameters:
  ///   - frame: The frame, in points from the display's top-left.
  ///   - display: The display; nil gives none.
  /// - Returns: The shares, or nil.
  static func share(_ frame: ActionFacts.Frame, of display: ActionFacts.Size?) -> CGRect? {
    guard let display else { return nil }
    let left = unit(frame.x / display.width)
    let top = unit(frame.y / display.height)
    let right = unit((frame.x + frame.width) / display.width)
    let bottom = unit((frame.y + frame.height) / display.height)
    guard right > left, bottom > top else { return nil }
    return CGRect(x: left, y: top, width: right - left, height: bottom - top)
  }

  /// A point on the display as a share of its width and height, kept on the picture.
  ///
  /// - Parameters:
  ///   - point: The point, in points from the display's top-left.
  ///   - display: The display; nil gives none.
  /// - Returns: The share, or nil.
  static func share(_ point: ActionFacts.Point, of display: ActionFacts.Size?) -> CGPoint? {
    guard let display else { return nil }
    return CGPoint(x: unit(point.x / display.width), y: unit(point.y / display.height))
  }

  /// A share kept between none and all.
  ///
  /// - Parameter share: The share.
  /// - Returns: It, from 0 to 1.
  private static func unit(_ share: Double) -> Double {
    min(max(share, 0), 1)
  }
}
