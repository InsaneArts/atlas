import AppKit
import AtlasUI

/// Moves the setup window to a new place on the screen in ``GlideSteps``: its content fades out, the window glides on
/// ``Motion/glideSoftCurve``, starting at once and landing softly, and the content fades back in near the end (v3 S7,
/// X13); under Reduce Motion the window fades out, lands and fades in. Until the latest move lands it counts as
/// gliding, so nothing else moves the window meanwhile.
///
/// A move supersedes the one before it: the earlier move's later steps do nothing, so Back during a fade out lands
/// the window where Back asked, and the newer move brings back whatever the earlier one left faded.
final class WindowGlide {
  /// The window it moves, which owns this glide and so outlives it.
  private unowned let window: NSWindow
  /// Whether Reduce Motion is on, read as each move starts.
  private let reduceMotion: @MainActor () -> Bool
  /// The animations each move is built from.
  private let motion: GlideMotion
  /// The number of the latest move: one with a lower number has been superseded.
  private var latest = 0
  /// The number of the last move that landed while it was still the latest.
  private var landed = 0
  /// Whether a move left the content faded out, with nothing under way to bring it back.
  private var contentFaded = false
  /// Whether a move left the whole window faded out, with nothing under way to bring it back.
  private var windowFaded = false
  /// Whether a move runs now.
  var isGliding: Bool { landed != latest }

  /// Creates the glide for `window`.
  ///
  /// - Parameters:
  ///   - window: The window it moves.
  ///   - reduceMotion: Whether Reduce Motion is on; the person's setting (``Appearance/reduceMotion``) unless a test
  ///     says.
  ///   - motion: The animations; AppKit's unless a test holds them.
  init(
    window: NSWindow,
    reduceMotion: @escaping @MainActor () -> Bool = { Appearance.reduceMotion },
    motion: GlideMotion = .appKit
  ) {
    self.window = window
    self.reduceMotion = reduceMotion
    self.motion = motion
  }

  /// Moves the window to `frame`: a glide, its content fading around it when the window changes size, or, under Reduce
  /// Motion, a fade out and in. A move still under way stops at its next step.
  ///
  /// - Parameter frame: Where it lands.
  func move(to frame: NSRect) {
    let steps = GlideSteps(reduceMotion: reduceMotion(), resizes: frame.size != window.frame.size)
    latest += 1
    let move = latest
    restore(except: steps.fading)
    switch steps.fading {
    case .none:
      motion.travel(window, frame, steps.travel) { [weak self] in self?.land(move) }
    case .content:
      contentFaded = true
      motion.fadeContent(window, 0, steps.fadeOut) { [weak self] in
        guard let self, latest == move else { return }
        contentFaded = false
        motion.travel(window, frame, steps.travel) { [weak self] in self?.land(move) }
        motion.reveal(window, steps.fadeInDelay, steps.fadeIn)
      }
    case .window:
      windowFaded = true
      motion.fadeWindow(window, 0, steps.fadeOut) { [weak self] in
        guard let self, latest == move else { return }
        windowFaded = false
        motion.travel(window, frame, steps.travel) {}
        motion.fadeWindow(window, 1, steps.fadeIn) { [weak self] in self?.land(move) }
      }
    }
  }

  /// Brings back what a superseded move left faded out, apart from what the move starting now fades itself.
  ///
  /// - Parameter fading: What the move starting now fades.
  private func restore(except fading: GlideSteps.Fading) {
    let fade = Motion.Duration.fade.seconds
    if contentFaded, fading != .content {
      contentFaded = false
      motion.fadeContent(window, 1, fade) {}
    }
    if windowFaded, fading != .window {
      windowFaded = false
      motion.fadeWindow(window, 1, fade) {}
    }
  }

  /// A move has landed: the window rests, unless a newer move has started since.
  ///
  /// - Parameter move: The move's number.
  private func land(_ move: Int) {
    if move == latest { landed = move }
  }
}
