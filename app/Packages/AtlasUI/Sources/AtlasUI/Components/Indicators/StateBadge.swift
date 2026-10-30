import SwiftUI

/// The small badge on a session icon's corner, saying what the session does now: an accent dot while it acts, a hollow
/// ring while it waits its turn, and the attention colour while it needs the person. A ring in the colour of the
/// ground it sits on cuts it out of the icon under it. Between two states its colours crossfade over
/// ``Motion/crossfade``; a badge arriving grows from a little under its size as it fades in, on ``Motion/arrive``, and
/// one leaving shrinks back as it fades out, on ``Motion/leave``. Under Reduce Motion it only fades and crossfades,
/// within ``Motion/Duration/reduced``. The words beside the icon say the same, so VoiceOver skips it.
struct StateBadge: View {
  /// What the session does now, or nil for no badge.
  let badge: SessionBadge?
  /// The ground the badge sits on, whose colour its ring takes.
  var ground = Color.groundRecessed
  @Environment(\.motionReduced) private var reduceMotion

  /// The badge in its state, or nothing, at a constant size either way so the icon beside it never moves.
  var body: some View {
    ZStack {
      if let badge {
        dot(badge)
          .transition(Motion.transition(arriving, reduceMotion: reduceMotion))
      }
    }
    .frame(width: StateBadgeLayout.reach, height: StateBadgeLayout.reach)
    .animation(Self.appearance(isShown: badge != nil, reduceMotion: reduceMotion), value: badge != nil)
    .accessibilityHidden(true)
  }

  /// The dot in its state's colours inside its ground ring, crossfading as the state changes.
  ///
  /// - Parameter badge: The state.
  /// - Returns: The dot.
  private func dot(_ badge: SessionBadge) -> some View {
    Circle()
      .fill(Self.fill(for: badge, ground: ground))
      .overlay {
        Circle()
          .strokeBorder(Self.innerRing(for: badge), lineWidth: StateBadgeLayout.ring)
      }
      .frame(width: StateBadgeLayout.side, height: StateBadgeLayout.side)
      .padding(StateBadgeLayout.ring)
      .background(
        Circle()
          .fill(ground)
      )
      .animation(Motion.crossfade, value: badge)
  }

  /// A badge growing in from a little under its size as it fades in, and shrinking back as it fades out.
  private var arriving: AnyTransition {
    AnyTransition.scale(scale: StateBadgeLayout.arrivingScale)
      .combined(with: .opacity)
  }

  /// The dot's fill: the accent while the session acts, the ground while it waits, and the attention colour while it
  /// needs the person.
  ///
  /// - Parameters:
  ///   - badge: The state.
  ///   - ground: The ground the badge sits on.
  /// - Returns: The fill.
  nonisolated static func fill(for badge: SessionBadge, ground: Color) -> Color {
    switch badge {
    case .acting: .accent
    case .waiting: ground
    case .needsYou: .attention
    }
  }

  /// The ring inside the dot's edge: tertiary while the session waits, so the hollow dot still reads, and clear
  /// otherwise.
  ///
  /// - Parameter badge: The state.
  /// - Returns: The ring's colour.
  nonisolated static func innerRing(for badge: SessionBadge) -> Color {
    badge == .waiting ? .textTertiary : .clear
  }

  /// How a badge comes and goes.
  ///
  /// - Parameters:
  ///   - isShown: Whether a badge shows now.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: ``Motion/arrive`` as it comes, ``Motion/leave`` as it goes, and the short fade ``Motion/reduced``
  ///   under Reduce Motion.
  nonisolated static func appearance(isShown: Bool, reduceMotion: Bool) -> Animation {
    Motion.animation(isShown ? Motion.arrive : Motion.leave, reduceMotion: reduceMotion, reduced: Motion.reduced)
  }
}

#if DEBUG
  #Preview("Samples") {
    StateBadgeSamples()
  }

  #Preview("Interactive") {
    @Previewable @State var badge: SessionBadge? = .acting
    InteractivePreview {
      Picker(String("State"), selection: $badge) {
        Text(verbatim: "None")
          .tag(SessionBadge?.none)
        ForEach(SessionBadge.allCases, id: \.self) { badge in
          Text(verbatim: String(describing: badge))
            .tag(SessionBadge?.some(badge))
        }
      }
      .pickerStyle(.segmented)
      .fixedSize()
    } content: {
      StateBadgeSamples([badge])
    }
  }
#endif
