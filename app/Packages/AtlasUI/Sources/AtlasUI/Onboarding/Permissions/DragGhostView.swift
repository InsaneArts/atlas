import SwiftUI

/// Atlas's icon, see-through, carried by a hand from the permission panel into System Settings' list, a few times:
/// how to allow Atlas, shown rather than told. Under Reduce Motion there is no flight, only a still hint resting where
/// it lands. It takes no clicks, and VoiceOver skips it, as the panel says the same in words.
public struct DragGhostView: View {
  /// Where it travels.
  private let state: DragGhostState
  /// The one moment of its loop it is drawn at, for a render; nil while it plays.
  private let moment: TimeInterval?
  /// Whether a render draws it as Reduce Motion would, whatever the setting.
  private let drawsStill: Bool
  @Environment(\.accessibilityReduceMotion) private var reducesMotion

  /// Creates the ghost.
  ///
  /// - Parameter state: Where it travels.
  public init(state: DragGhostState) {
    self.state = state
    self.moment = nil
    self.drawsStill = false
  }

  /// Whether Reduce Motion is on, or a render draws as if it were.
  private var reduceMotion: Bool {
    reducesMotion || drawsStill
  }

  /// The ghost at this moment of its loop, or resting at the list under Reduce Motion.
  public var body: some View {
    Group {
      if let moment {
        ghost(Self.track(at: moment, reduceMotion: reduceMotion))
      } else if Motion.loops(reduceMotion: reduceMotion) {
        TimelineView(.animation(minimumInterval: DragGhostTrack.frameInterval)) { context in
          ghost(Self.track(at: context.date.timeIntervalSince(state.start), reduceMotion: reduceMotion))
        }
      } else {
        ghost(.landed)
      }
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .allowsHitTesting(false)
    .accessibilityHidden(true)
  }

  /// The ghost `time` into its loop: travelling from the icon to the list, or, under Reduce Motion, the still hint
  /// at the list whatever the time, as nothing loops then.
  ///
  /// - Parameters:
  ///   - time: Seconds since its loop began.
  ///   - reduceMotion: Whether Reduce Motion is on.
  /// - Returns: The ghost then.
  static func track(at time: TimeInterval, reduceMotion: Bool) -> DragGhostTrack {
    Motion.loops(reduceMotion: reduceMotion) ? .at(time) : .landed
  }

  /// The icon by its tile and the hand holding it, dark with a light edge as the pointer is, so it shows over light
  /// and dark lists alike, at `track`'s place, size and opacity, see-through as a demonstration is.
  ///
  /// - Parameter track: The moment of the loop.
  /// - Returns: The ghost.
  private func ghost(_ track: DragGhostTrack) -> some View {
    ZStack(alignment: .bottomTrailing) {
      AppIcon(tile: OnboardingLayout.dragTile)
      ZStack {
        Image(systemName: SymbolName.handFill.rawValue)
          .foregroundStyle(Color.fillHand)
        Image(systemName: SymbolName.hand.rawValue)
          .foregroundStyle(Color.onAccent)
      }
      .font(OnboardingLayout.ghostHandFont)
      .elevation(.floating)
      .offset(x: OnboardingLayout.ghostHandOffset, y: OnboardingLayout.ghostHandOffset)
    }
    .scaleEffect(track.scale)
    .opacity(track.opacity * Opacity.muted)
    .position(state.point(at: track.progress))
  }
}

#if DEBUG
  extension DragGhostView {
    /// Creates the ghost drawn at one moment of its loop, as renders draw it.
    ///
    /// - Parameters:
    ///   - state: Where it travels.
    ///   - moment: Seconds into its loop.
    ///   - reduceMotion: Whether to draw it as Reduce Motion would; false by default.
    public init(state: DragGhostState, at moment: TimeInterval, reduceMotion: Bool = false) {
      self.state = state
      self.moment = moment
      self.drawsStill = reduceMotion
    }
  }

  #Preview("Drag ghost") {
    DragGhostView(state: DragGhostState(from: CGPoint(x: 80, y: 200), to: CGPoint(x: 420, y: 140), start: .now))
      .frame(width: SampleWidth.column, height: OnboardingLayout.side.height)
      .sampleSurface()
  }
#endif
