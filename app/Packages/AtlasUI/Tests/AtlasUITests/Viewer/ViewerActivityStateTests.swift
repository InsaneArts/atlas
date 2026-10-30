import Testing

@testable import AtlasUI

@Suite("Nested Viewer Activity")
struct ViewerActivityStateTests {
  @MainActor
  @Test("a moment's steps can only exist inside their moment")
  func stepsBelongToTheirMoment() {
    var state = PreviewStates.workingActivity
    let moment = state.days[0].moments.removeFirst()
    #expect(moment.steps.map(\.id) == ["o6", "o5", "o4", "o3", "o2", "o1"])
    let remaining = state.days
      .flatMap(\.moments)
      .flatMap(\.steps)
    #expect(Set(remaining.map(\.id)).isDisjoint(with: moment.steps.map(\.id)))
    #expect(state.newest == "n3", "removing a moment removes its newest step too")
    state.days.removeAll()
    #expect(state.newest == nil)
  }

  @MainActor
  @Test("nested days retain heading moment step and Show more ordering")
  func nestedContentKeepsItsOrder() {
    let days = PreviewStates.daysActivity.days
    #expect(days.map(\.title) == ["Yesterday", "Di. 20. Okt."])
    #expect(days.map { $0.moments.map(\.title) } == [["Safari"], ["BuildTool"]])
    let opened = PreviewStates.openedActivity.days[0].moments
    #expect(opened.map(\.title) == ["Opie", "Notes", "TextEdit", "Commands"])
    #expect(opened[2].steps.count == 8)
    #expect(opened[2].steps.last?.isLast == true)
    #expect(opened[2].more == ViewerMoreSteps(moment: opened[2].id, count: 1))
    #expect(opened[1].steps.isEmpty)
  }
}
