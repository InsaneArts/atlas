import Foundation

/// The drag ghost at one moment of its loop: how far it has come from the panel's icon to the list, how much of it
/// shows, and its size. Over 3 seconds it appears, travels, lands shrinking as it fades, then rests unseen.
package struct DragGhostTrack: Equatable, Sendable {
  /// 0.3 s: appearing on the panel's icon.
  static let appear: TimeInterval = 0.3
  /// 1.6 s: having travelled to the list.
  static let travel: TimeInterval = 1.6
  /// 2.0 s: having landed and faded.
  package static let land: TimeInterval = 2.0
  /// 3.0 s: the whole loop, the rest unseen.
  package static let cycle: TimeInterval = 3.0
  /// 3: the loops it shows for one permission before it rests, so it shows the way without going on and on.
  package static let loops = 3
  /// 0.15: how much smaller it ends as it lands, as if dropped into the list.
  static let landShrink = 0.15
  /// A thirtieth of a second: how often it is drawn, smooth enough for a demonstration, and light while it runs.
  package static let frameInterval: TimeInterval = 1.0 / 30

  /// How far along from the panel's icon to the list, from 0 to 1.
  package var progress: Double
  /// How much of it shows, from 0 to 1.
  package var opacity: Double
  /// Its size, 1 being the panel's icon's.
  package var scale: Double

  /// Creates one moment.
  ///
  /// - Parameters:
  ///   - progress: How far along, from 0 to 1.
  ///   - opacity: How much shows, from 0 to 1.
  ///   - scale: Its size.
  package init(progress: Double, opacity: Double, scale: Double) {
    self.progress = progress
    self.opacity = opacity
    self.scale = scale
  }

  /// Where it lands, still: what Reduce Motion shows.
  package static let landed = DragGhostTrack(progress: 1, opacity: 1, scale: 1)

  /// The moment of the loop at `time`.
  ///
  /// - Parameter time: Seconds on any clock; the loop repeats every ``cycle``.
  /// - Returns: The ghost then.
  package static func at(_ time: TimeInterval) -> DragGhostTrack {
    let remainder = time.truncatingRemainder(dividingBy: cycle)
    let moment = remainder < 0 ? remainder + cycle : remainder
    switch moment {
    case ..<appear:
      return DragGhostTrack(progress: 0, opacity: moment / appear, scale: 1)
    case ..<travel:
      return DragGhostTrack(progress: eased((moment - appear) / (travel - appear)), opacity: 1, scale: 1)
    case ..<land:
      let landing = (moment - travel) / (land - travel)
      return DragGhostTrack(progress: 1, opacity: 1 - landing, scale: 1 - landShrink * landing)
    default:
      return DragGhostTrack(progress: 1, opacity: 0, scale: 1 - landShrink)
    }
  }

  /// An ease in and out: slow off the icon, quick across, slow into the list.
  ///
  /// - Parameter linear: How far along in time, from 0 to 1.
  /// - Returns: How far along the path.
  static func eased(_ linear: Double) -> Double {
    linear < 0.5 ? 2 * linear * linear : 1 - pow(2 - 2 * linear, 2) / 2
  }
}
