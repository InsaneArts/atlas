import SwiftUI

/// A moment's header, one view whether open or folded, so it changes in place: the app's icon in the column the steps'
/// discs run down, its title, whose work it was while every session's work shows, and when. Open, it says when the
/// moment started and its line starts under the icon; folded, it is a true capsule that says how much it held and
/// when, its age at the end. The chevron sits at the end, so icons stay in one column, and turns as the moment opens.
/// A moment held open, as while a step runs, shows no chevron and its press does nothing; once it can fold, the same
/// header shows its chevron in place.
struct ActivityMomentRow: View {
  /// The moment.
  let moment: ViewerActivityMoment
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The header, one button whether or not the moment can fold, so a moment that comes to rest keeps its one header
  /// where it stands; held open, its press does nothing and its capsule never shows.
  var body: some View {
    Button {
      guard moment.canFold else { return }
      withAnimation(ActivityMotion.fold(reduceMotion: reduceMotion)) {
        send(.toggleMoment(moment.id, opens: !moment.isOpen))
      }
    } label: {
      label
    }
    .buttonStyle(ActivityMomentStyle(isOpen: moment.isOpen, canFold: moment.canFold))
    .accessibilityHint(hint)
    .padding(.horizontal, -ViewerLayout.capsuleReach)
    .background(alignment: .bottomLeading) {
      if moment.isOpen {
        ActivityTimelineSegment()
          .frame(height: ActivityMomentRowLayout.lineUnderIcon)
          .transition(.opacity)
      }
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .accessibilityElement(children: .combine)
    .accessibilityLabel(moment.label)
    .accessibilityAddTraits(.isHeader)
    .accessibilityRemoveTraits(moment.canFold ? [] : .isButton)
  }

  /// What pressing the header does, for VoiceOver; nothing while the moment is held open.
  private var hint: Text {
    guard moment.canFold else { return Text(verbatim: "") }
    return moment.isOpen ? Text("Folds this moment") : Text("Shows this moment's steps")
  }

  /// The icon, the title, the session and the time or the span, the age while folded, and the chevron.
  private var label: some View {
    HStack(spacing: Spacing.xsPlus) {
      ActivityGlyph(icon: moment.icon, side: ViewerLayout.appIcon)
        .padding(.trailing, ViewerLayout.momentIconGap - Spacing.xsPlus)
      Text(verbatim: moment.title)
        .font(Typography.font(.body, weight: .semibold))
        .foregroundStyle(Color.textPrimary)
        .layoutPriority(1)
      if let session = moment.session {
        dot
        sessionLabel(session)
      }
      dot
      meta
      Spacer(minLength: Spacing.s)
      if !moment.isOpen {
        Text(verbatim: moment.age)
          .font(Typography.digits(.callout))
          .foregroundStyle(Color.textTertiary)
          .contentTransition(.identity)
          .fixedSize()
          .transition(.opacity)
      }
      chevron
        .opacity(moment.canFold ? 1 : 0)
    }
    .lineLimit(1)
  }

  /// When the moment started while open, or how much it held and when while folded: both lines stand in one place,
  /// each always drawn with its own words, and as the moment opens or folds one fades out before the other fades in,
  /// so the words never overlap and, drawn all along, travel with the header when it moves.
  private var meta: some View {
    ZStack(alignment: .leading) {
      metaLine(moment.span, isShown: !moment.isOpen)
      metaLine(moment.time, isShown: moment.isOpen)
    }
  }

  /// One of the meta's lines, faded in or out as the moment opens or folds.
  ///
  /// - Parameters:
  ///   - words: The line.
  ///   - isShown: Whether it shows.
  /// - Returns: The line.
  private func metaLine(_ words: String, isShown: Bool) -> some View {
    Text(verbatim: words)
      .font(Typography.digits(.callout))
      .foregroundStyle(Color.textTertiary)
      .contentTransition(.identity)
      // Only the line's opacity takes the swap's fade, so it moves with the header as the rest of it does.
      .animation(isShown ? Motion.swapInAfterOut : Motion.swapOut) { line in
        line.opacity(isShown ? 1 : 0)
      }
      .accessibilityHidden(!isShown)
  }

  /// A quiet middle dot between the header's parts.
  private var dot: some View {
    Text(verbatim: "·")
      .font(Typography.font(.callout))
      .foregroundStyle(Color.textTertiary)
  }

  /// Whose work it was: the host's icon and the session's name, or the person's mark and "You".
  ///
  /// - Parameter session: The session.
  /// - Returns: The label.
  private func sessionLabel(_ session: ViewerMomentSession) -> some View {
    HStack(spacing: Spacing.xs) {
      if let icon = session.icon {
        SessionIcon(icon: icon, side: ViewerLayout.momentSessionIcon)
      } else {
        ActivityGlyph(icon: .you, side: ViewerLayout.momentSessionIcon)
      }
      Text(verbatim: session.name)
        .font(Typography.font(.callout))
        .foregroundStyle(Color.textSecondary)
        .truncationMode(.tail)
    }
  }

  /// The chevron, pointing at the steps while open; under Reduce Motion it swaps rather than turns.
  private var chevron: some View {
    Group {
      if reduceMotion {
        Image(systemName: moment.isOpen ? SymbolName.menu.rawValue : SymbolName.next.rawValue)
      } else {
        // Only the turn takes the header's animation, so the chevron moves with the header as the rest of it does.
        Image(systemName: SymbolName.next.rawValue)
          .animation(ActivityMotion.header(reduceMotion: reduceMotion)) { chevron in
            chevron.rotationEffect(moment.isOpen ? ActivityMomentRowLayout.turned : .zero)
          }
      }
    }
    .font(Typography.font(.caption, weight: .semibold))
    .foregroundStyle(Color.textTertiary)
    .frame(width: ActivityMomentRowLayout.chevron)
    .accessibilityHidden(true)
  }
}

#if DEBUG
  #Preview("Moments") {
    ActivitySamples(.moments)
  }

  #Preview("Interactive") {
    ActivitySamples(.folding)
  }
#endif
