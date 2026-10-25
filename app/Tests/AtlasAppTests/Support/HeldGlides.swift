import AppKit

@testable import AtlasApp

/// A glide's animations held until the test ends them, in any order: a fade sets its opacity at once, as AppKit's
/// animator sets the value it animates to, and a travel puts the window at its frame only as it ends.
@MainActor
final class HeldGlides {
  /// The frames the window was asked to travel to, in order.
  private(set) var travels: [NSRect] = []
  /// The opacities the content was asked to fade to, in order, a reveal's 1 among them.
  private(set) var contentFades: [CGFloat] = []
  /// The animations asked for and not ended yet, oldest first.
  private var held: [@MainActor () -> Void] = []

  /// Whether an animation waits to be ended.
  var isHolding: Bool { !held.isEmpty }

  /// The animations, held here.
  var motion: GlideMotion {
    GlideMotion(
      fadeContent: { window, alpha, _, done in
        self.contentFades.append(alpha)
        window.contentView?.alphaValue = alpha
        self.held.append(done)
      },
      fadeWindow: { window, alpha, _, done in
        window.alphaValue = alpha
        self.held.append(done)
      },
      travel: { window, frame, _, done in
        self.travels.append(frame)
        self.held.append {
          window.setFrame(frame, display: false)
          done()
        }
      },
      reveal: { window, _, _ in
        self.contentFades.append(1)
        window.contentView?.alphaValue = 1
      }
    )
  }

  /// Ends the oldest animation still held.
  func endOldest() {
    guard !held.isEmpty else { return }
    held.removeFirst()()
  }

  /// Ends every animation held, oldest first, and those their ends ask for.
  func endAll() {
    while isHolding { endOldest() }
  }
}
