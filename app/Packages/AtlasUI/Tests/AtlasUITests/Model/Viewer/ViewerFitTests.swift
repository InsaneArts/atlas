import CoreGraphics
import Testing

@testable import AtlasUI

@MainActor
@Suite("ViewerFit")
struct ViewerFitTests {
  /// A window of `width` at the standard height.
  func wide(_ width: CGFloat) -> CGSize {
    CGSize(width: width, height: PreviewStates.viewerSize.height)
  }

  @Test("the sidebar hides under 688 and shows again at 700")
  func theSidebarHidesAndShowsAgain() {
    #expect(ViewerSidebarMode.fitting(windowWidth: 700, current: .full) == .full)
    #expect(ViewerSidebarMode.fitting(windowWidth: 687, current: .full) == .hidden)
    #expect(ViewerSidebarMode.fitting(windowWidth: 320, current: .full) == .hidden)
    #expect(ViewerSidebarMode.fitting(windowWidth: 699, current: .hidden) == .hidden)
    #expect(ViewerSidebarMode.fitting(windowWidth: 700, current: .hidden) == .full)
    #expect(ViewerSidebarMode.fitting(windowWidth: 1200, current: .hidden) == .full)
  }

  @Test("near the threshold the sidebar keeps its mode within 12 points")
  func nearTheThresholdTheSidebarKeepsItsMode() {
    for width in stride(from: CGFloat(688), to: 700, by: 1) {
      #expect(ViewerSidebarMode.fitting(windowWidth: width, current: .full) == .full)
      #expect(ViewerSidebarMode.fitting(windowWidth: width, current: .hidden) == .hidden)
    }
  }

  @Test("the sidebar leaves the card room for the whole footer at every width it shows beside")
  func theSidebarNeverCrampsTheCard() {
    let card = ViewerLayout.cardWidth(windowWidth: 688, mode: .full)
    #expect(card == 444)
    #expect(ViewerFooterFit.fitting(cardWidth: card, current: .full) == .full)
    #expect(ViewerFooterFit.fitting(cardWidth: card, current: .compact) == .full)
  }

  @Test("the footer's capsules turn into circles below 408 and keep their words again from 420")
  func theFootersCapsulesTurnIntoCircles() {
    #expect(ViewerFooterFit.fitting(cardWidth: 408, current: .full) == .full)
    #expect(ViewerFooterFit.fitting(cardWidth: 407, current: .full) == .compact)
    #expect(ViewerFooterFit.fitting(cardWidth: 419, current: .compact) == .compact)
    #expect(ViewerFooterFit.fitting(cardWidth: 420, current: .compact) == .full)
    #expect(!ViewerFooterFit.full.isCircles)
    #expect(ViewerFooterFit.compact.isCircles && ViewerFooterFit.compact.showsWords)
  }

  @Test("below 260 the footer's line keeps only its icon and its age")
  func belowTwoSixtyTheLineKeepsItsIconAndAge() {
    #expect(ViewerFooterFit.fitting(cardWidth: 259, current: .compact) == .tight)
    #expect(ViewerFooterFit.fitting(cardWidth: 271, current: .tight) == .tight)
    #expect(ViewerFooterFit.fitting(cardWidth: 272, current: .tight) == .compact)
    #expect(!ViewerFooterFit.tight.showsWords)
  }

  @Test("each part gives way to the window as it shrinks and comes back as it grows")
  func eachPartGivesWayToTheWindow() {
    let first = ViewerFit()
    let large = first.refitted(to: wide(1200), isHidden: false)
    #expect(large == ViewerFit())
    let narrow = large.refitted(to: wide(600), isHidden: false)
    #expect(narrow.sidebar == .hidden)
    #expect(narrow.footer == .full, "hidden, the sidebar leaves the card 584")
    let compact = narrow.refitted(to: wide(400), isHidden: false)
    #expect(compact.footer == .compact, "the card's 384 turns the footer's capsules into circles")
    #expect(compact.showsWhat)
    let small = compact.refitted(to: wide(320), isHidden: false)
    #expect(small.sidebar == .hidden)
    #expect(!small.showsWhat, "the card's 304 leaves the header to its name")
    let short = small.refitted(to: CGSize(width: 1200, height: 220), isHidden: false)
    #expect(short.sidebar == .full)
    #expect(short.footer == .full)
    #expect(short.isShort)
  }

  @Test("the person's hidden sidebar gives the card the window's room, without changing what the width says")
  func thePersonsHiddenSidebarGivesTheCardItsRoom() {
    let first = ViewerFit()
    let fit = first.refitted(to: wide(700), isHidden: true)
    #expect(fit.sidebar == .full, "the width's own say, for when the person shows it again")
    #expect(ViewerLayout.cardWidth(windowWidth: 700, mode: .hidden) == 684, "hidden, the card has the window")
  }

  @Test("a fitted state keeps the person's hidden sidebar and shows it over the card only while it cannot fit whole")
  func aFittedStateKeepsThePersonsChoice() {
    let hiddenSample = PreviewStates.viewerSidebarHidden
    let hidden = hiddenSample.fitted(ViewerFit(sidebar: .full))
    #expect(hidden.sidebar.mode == .hidden)
    let overlay = PreviewStates.viewerSidebarOverlay
    #expect(overlay.fitted(ViewerFit(sidebar: .hidden)).sidebar.overlay)
    #expect(!overlay.fitted(ViewerFit()).sidebar.overlay)
    let working = PreviewStates.viewerWorking
    let narrow = working.fitted(ViewerFit(sidebar: .hidden, footer: .tight, showsWhat: false))
    #expect(narrow.sidebar.mode == .hidden)
    #expect(narrow.footer.fit == .tight)
    #expect(!narrow.header.showsWhat)
  }
}
