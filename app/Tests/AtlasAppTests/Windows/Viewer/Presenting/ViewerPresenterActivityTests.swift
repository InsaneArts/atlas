import AtlasService
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("ViewerPresenter+Activity")
struct ViewerPresenterActivityTests {
  @Test("each kind of action has its symbol, and a problem has the warning symbol")
  func eachKindHasItsSymbol() {
    let lines: [(entry: ActivityLog.Entry, symbol: SymbolName)] = [
      (.init(id: "1", says: "clicked Save in TextEdit"), .click),
      (.init(id: "2", says: "typed in TextEdit"), .keyboard),
      (.init(id: "3", says: "scrolled in Opie"), .scroll),
      (.init(id: "4", says: "took a screenshot", operation: "screenshot"), .camera),
      (.init(id: "5", says: "opened Opie", operation: "run"), .window),
      (.init(id: "6", says: "ran a command", operation: "run"), .terminal),
      (.init(id: "7", says: "clicked in Opie", actor: .you), .person),
      (.init(id: "8", says: "gave back control", actor: .you, operation: "release"), .handRaised),
      (.init(id: "9", says: "couldn't click Save in TextEdit", outcome: .failed), .problem),
      (.init(id: "10", says: "couldn't type", outcome: .failed, actor: .you), .problem),
    ]
    for line in lines {
      #expect(ViewerPresenter.kind(line.entry).symbol == line.symbol, "\(line.entry.id)")
    }
    // A problem's tile is orange as well as its symbol a warning; the person's own is blue.
    #expect(ViewerPresenter.lead(lines[8].entry) == .failed)
    #expect(ViewerPresenter.lead(lines[6].entry) == .person)
    // No two kinds share a symbol, so the kind always reads.
    let symbols = Set(ViewerActivityKind.allCases.map(\.symbol))
    #expect(symbols.count == ViewerActivityKind.allCases.count)
  }
}
