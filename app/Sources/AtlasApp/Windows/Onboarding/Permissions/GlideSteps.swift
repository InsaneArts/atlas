import AtlasUI
import Foundation

/// How the setup window moves to a new place (v3 S7, X13), as times: its content fades out, the window glides on
/// ``Motion/glideSoftCurve``, and the content fades back in from 80% of the way, so nothing reflows on the way. A move
/// that keeps the window's size glides alone, as nothing reflows. Under Reduce Motion nothing travels: the window
/// fades out, lands, and fades in.
struct GlideSteps: Equatable {
  /// What fades around the move.
  enum Fading: Equatable {
    /// Nothing: the window glides with its content shown.
    case none
    /// The window's content, while the window glides.
    case content
    /// The whole window, which lands at once in between.
    case window
  }

  /// What fades around the move.
  let fading: Fading
  /// How long the fade out takes before the window moves, in seconds.
  let fadeOut: TimeInterval
  /// How long the window travels, in seconds; 0 puts it there at once.
  let travel: TimeInterval
  /// How long after the window starts to move the fade in starts, in seconds.
  let fadeInDelay: TimeInterval
  /// How long the fade in takes, in seconds.
  let fadeIn: TimeInterval

  /// The steps of one move.
  ///
  /// - Parameters:
  ///   - reduceMotion: Whether Reduce Motion is on.
  ///   - resizes: Whether the window changes size on the way, between the card and the panel.
  init(reduceMotion: Bool, resizes: Bool) {
    let glide = Motion.Duration.glideSoft.seconds
    switch (reduceMotion, resizes) {
    case (true, _):
      self.init(fading: .window, fadeOut: Motion.Duration.contentOut.seconds, travel: 0, fadeInDelay: 0)
    case (false, true):
      self.init(
        fading: .content,
        fadeOut: Motion.Duration.contentOut.seconds,
        travel: glide,
        fadeInDelay: glide * Motion.glideReveal
      )
    case (false, false):
      self.init(fading: .none, fadeOut: 0, travel: glide, fadeInDelay: 0)
    }
  }

  /// The steps as given; the fade in takes ``Motion/Duration/fade``.
  ///
  /// - Parameters:
  ///   - fading: What fades around the move.
  ///   - fadeOut: How long the fade out takes.
  ///   - travel: How long the window travels.
  ///   - fadeInDelay: How long after the window starts to move the fade in starts.
  private init(fading: Fading, fadeOut: TimeInterval, travel: TimeInterval, fadeInDelay: TimeInterval) {
    self.fading = fading
    self.fadeOut = fadeOut
    self.travel = travel
    self.fadeInDelay = fadeInDelay
    self.fadeIn = fading == .none ? 0 : Motion.Duration.fade.seconds
  }
}
